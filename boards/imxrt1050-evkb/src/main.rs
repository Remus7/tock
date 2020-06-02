//! Board file for Nucleo-F429ZI development board
//!
//! - <https://www.st.com/en/evaluation-tools/nucleo-f429zi.html>

#![no_std]
#![no_main]
#![feature(asm)]
#![deny(missing_docs)]

use capsules::virtual_alarm::{MuxAlarm, VirtualMuxAlarm};
// use components::gpio::GpioComponent;
use kernel::capabilities;
use kernel::common::dynamic_deferred_call::{DynamicDeferredCall, DynamicDeferredCallClientState};
use kernel::component::Component;
use kernel::hil::time::Alarm;
use kernel::Platform;
use kernel::{create_capability, debug, static_init};
use kernel::hil::gpio::Configure;
use kernel::hil::gpio::Output;

// Unit Tests for drivers.
// #[allow(dead_code)]
// mod virtual_uart_rx_test;

/// Support routines for debugging I/O.
pub mod io;

// Number of concurrent processes this platform supports.
const NUM_PROCS: usize = 1;

// Actual memory for holding the active process structures.
static mut PROCESSES: [Option<&'static dyn kernel::procs::ProcessType>; NUM_PROCS] =
    [None];

static mut CHIP: Option<&'static imxrt1050::chip::Imxrt1050> = None;

// How should the kernel respond when a process faults.
const FAULT_RESPONSE: kernel::procs::FaultResponse = kernel::procs::FaultResponse::Panic;

// RAM to be shared by all application processes.
#[link_section = ".app_memory"]
static mut APP_MEMORY: [u8; 65536] = [0; 65536];

// Force the emission of the `.apps` segment in the kernel elf image
// NOTE: This will cause the kernel to overwrite any existing apps when flashed!
#[used]
#[link_section = ".app.hack"]
static APP_HACK: u8 = 0;

/// Dummy buffer that causes the linker to reserve enough space for the stack.
#[no_mangle]
#[link_section = ".stack_buffer"]
pub static mut STACK_MEMORY: [u8; 0x1000] = [0; 0x1000];

const NUM_LEDS: usize = 1;

/// A structure representing this platform that holds references to all
/// capsules for this platform.
struct Imxrt1050EVKB {
    console: &'static capsules::console::Console<'static>,
    ipc: kernel::ipc::IPC,
    led: &'static capsules::led::LED<'static>,
    // button: &'static capsules::button::Button<'static>,
    alarm: &'static capsules::alarm::AlarmDriver<
        'static,
        VirtualMuxAlarm<'static, imxrt1050::gpt1::Gpt1<'static>>,
    >,
    // gpio: &'static capsules::gpio::GPIO<'static>,
}

/// Mapping of integer syscalls to objects that implement syscalls.
impl Platform for Imxrt1050EVKB {
    fn with_driver<F, R>(&self, driver_num: usize, f: F) -> R
    where
        F: FnOnce(Option<&dyn kernel::Driver>) -> R,
    {
        match driver_num {
            capsules::console::DRIVER_NUM => f(Some(self.console)),
            capsules::led::DRIVER_NUM => f(Some(self.led)),
            // capsules::button::DRIVER_NUM => f(Some(self.button)),
            capsules::alarm::DRIVER_NUM => f(Some(self.alarm)),
            kernel::ipc::DRIVER_NUM => f(Some(&self.ipc)),
            // capsules::gpio::DRIVER_NUM => f(Some(self.gpio)),
            _ => f(None),
        }
    }
}

/// Helper function called during bring-up that configures DMA.
// unsafe fn setup_dma() {
//     use imxr::dma1::{Dma1Peripheral, DMA1};
//     use stm32f4xx::usart;
//     use stm32f4xx::usart::USART3;

//     DMA1.enable_clock();

//     let usart3_tx_stream = Dma1Peripheral::USART3_TX.get_stream();
//     let usart3_rx_stream = Dma1Peripheral::USART3_RX.get_stream();

//     USART3.set_dma(
//         usart::TxDMA(usart3_tx_stream),
//         usart::RxDMA(usart3_rx_stream),
//     );

//     usart3_tx_stream.set_client(&USART3);
//     usart3_rx_stream.set_client(&USART3);

//     usart3_tx_stream.setup(Dma1Peripheral::USART3_TX);
//     usart3_rx_stream.setup(Dma1Peripheral::USART3_RX);

//     cortexm4::nvic::Nvic::new(Dma1Peripheral::USART3_TX.get_stream_irqn()).enable();
//     cortexm4::nvic::Nvic::new(Dma1Peripheral::USART3_RX.get_stream_irqn()).enable();
// }

/// Helper function called during bring-up that configures multiplexed I/O.
unsafe fn set_pin_primary_functions() {
    // use kernel::hil::gpio::Configure;
    // use stm32f4xx::exti::{LineId, EXTI};
    use imxrt1050::gpio::{AlternateFunction, Mode, PinId, PortId, PORT};
    // use stm32f4xx::syscfg::SYSCFG;
    use imxrt1050::ccm::CCM;

    CCM.enable_iomuxc_clock();
    CCM.enable_gpio1_clock();
    // SYSCFG.enable_clock();

    PORT[PortId::P1 as usize].enable_clock();

    // User_LED is connected to P1_09. Configure P1_09 as `debug_gpio!(0, ...)`
    PinId::P1_09.get_pin().as_ref().map(|pin| {
        pin.make_output();

        // Configure kernel debug gpios as early as possible
        kernel::debug::assign_gpios(Some(pin), None, None);
    });

    // PORT[PortId::D as usize].enable_clock();

    // // pd8 and pd9 (USART3) is connected to ST-LINK virtual COM port
    // PinId::PD08.get_pin().as_ref().map(|pin| {
    //     pin.set_mode(Mode::AlternateFunctionMode);
    //     // AF7 is USART2_TX
    //     pin.set_alternate_function(AlternateFunction::AF7);
    // });
    // PinId::PD09.get_pin().as_ref().map(|pin| {
    //     pin.set_mode(Mode::AlternateFunctionMode);
    //     // AF7 is USART2_RX
    //     pin.set_alternate_function(AlternateFunction::AF7);
    // });

    // PORT[PortId::C as usize].enable_clock();

    // // button is connected on pc13
    // PinId::PC13.get_pin().as_ref().map(|pin| {
    //     // By default, upon reset, the pin is in input mode, with no internal
    //     // pull-up, no internal pull-down (i.e., floating).
    //     //
    //     // Only set the mapping between EXTI line and the Pin and let capsule do
    //     // the rest.
    //     EXTI.associate_line_gpiopin(LineId::Exti13, pin);
    // });
    // // EXTI13 interrupts is delivered at IRQn 40 (EXTI15_10)
    // cortexm4::nvic::Nvic::new(stm32f4xx::nvic::EXTI15_10).enable();

    // // Enable clocks for GPIO Ports
    // // Disable some of them if you don't need some of the GPIOs
    // PORT[PortId::P1 as usize].enable_clock();
    // // Ports B, C and D are already enabled
    // PORT[PortId::E as usize].enable_clock();
    // PORT[PortId::F as usize].enable_clock();
    // PORT[PortId::G as usize].enable_clock();
    // PORT[PortId::H as usize].enable_clock();
}

/// Helper function for miscellaneous peripheral functions
unsafe fn setup_peripherals() {
    use imxrt1050::gpt1::GPT1;

    // USART3 IRQn is 39
    // cortexm7::nvic::Nvic::new(stm32f4xx::nvic::USART3).enable();

    // LPUART1 IRQn is 20
    cortexm7::nvic::Nvic::new(imxrt1050::nvic::LPUART1).enable();

    // TIM2 IRQn is 28
    GPT1.enable_clock();
    GPT1.start();
    cortexm7::nvic::Nvic::new(imxrt1050::nvic::GPT1).enable();
}

/// Reset Handler.
///
/// This symbol is loaded into vector table by the STM32F446RE chip crate.
/// When the chip first powers on or later does a hard reset, after the core
/// initializes all the hardware, the address of this function is loaded and
/// execution begins here.
#[no_mangle]
pub unsafe fn reset_handler() {
    debug!("Booting TockOS!!");
    imxrt1050::init();
    imxrt1050::lpuart::LPUART1.set_baud();

    // We use the default HSI 16Mhz clock

    set_pin_primary_functions();

    // setup_dma();

    setup_peripherals();

    let board_kernel = static_init!(kernel::Kernel, kernel::Kernel::new(&PROCESSES));

    let dynamic_deferred_call_clients =
        static_init!([DynamicDeferredCallClientState; 2], Default::default());
    let dynamic_deferred_caller = static_init!(
        DynamicDeferredCall,
        DynamicDeferredCall::new(dynamic_deferred_call_clients)
    );
    DynamicDeferredCall::set_global_instance(dynamic_deferred_caller);

    let chip = static_init!(
        imxrt1050::chip::Imxrt1050,
        imxrt1050::chip::Imxrt1050::new()
    );
    CHIP = Some(chip);

    // LPUART
    // Enable clock
    // imxrt1050::lpuart::LPUART1.enable_clock();

    // Enable tx and rx from iomuxc
    imxrt1050::iomuxc::IOMUXC.enable_lpuart1_tx();
    imxrt1050::iomuxc::IOMUXC.enable_lpuart1_rx();
    imxrt1050::iomuxc::IOMUXC.set_pin_config_lpuart1();

    let lpuart_mux = components::console::UartMuxComponent::new(
       &imxrt1050::lpuart::LPUART1,
        115200,
        dynamic_deferred_caller,
    )
    .finalize(());
    io::WRITER.set_initialized();

    // UART

    // Create a shared UART channel for kernel debug.
    // stm32f4xx::usart::USART3.enable_clock();

    // let uart_mux = components::console::UartMuxComponent::new(
    //     &imxrt1050::usart::USART_SEMIHOSTING,
    //     115200,
    //     dynamic_deferred_caller,
    // )
    // .finalize(());

    // io::WRITER.set_initialized();

    // Create capabilities that the board needs to call certain protected kernel
    // functions.
    let memory_allocation_capability = create_capability!(capabilities::MemoryAllocationCapability);
    let main_loop_capability = create_capability!(capabilities::MainLoopCapability);
    let process_management_capability =
        create_capability!(capabilities::ProcessManagementCapability);

    // Setup the console.
    let console = components::console::ConsoleComponent::new(board_kernel, lpuart_mux).finalize(());
    // Create the debugger object that handles calls to `debug!()`.
    components::debug_writer::DebugWriterComponent::new(lpuart_mux).finalize(());

    // // Setup the process inspection console
    // let process_console_uart = static_init!(UartDevice, UartDevice::new(mux_uart, true));
    // process_console_uart.setup();
    // pub struct ProcessConsoleCapability;
    // unsafe impl capabilities::ProcessManagementCapability for ProcessConsoleCapability {}
    // let process_console = static_init!(
    //     capsules::process_console::ProcessConsole<'static, ProcessConsoleCapability>,
    //     capsules::process_console::ProcessConsole::new(
    //         process_console_uart,
    //         &mut capsules::process_console::WRITE_BUF,
    //         &mut capsules::process_console::READ_BUF,
    //         &mut capsules::process_console::COMMAND_BUF,
    //         board_kernel,
    //         ProcessConsoleCapability,
    //     )
    // );
    // hil::uart::Transmit::set_transmit_client(process_console_uart, process_console);
    // hil::uart::Receive::set_receive_client(process_console_uart, process_console);
    // process_console.start();

    // LEDs

    // Clock to Port A is enabled in `set_pin_primary_functions()`
    let led_pins = static_init!(
        [(
            &'static dyn kernel::hil::gpio::Pin,
            capsules::led::ActivationMode
        ); NUM_LEDS],
        [
            (
                imxrt1050::gpio::PinId::P1_09.get_pin().as_ref().unwrap(),
                capsules::led::ActivationMode::ActiveLow
            )
        ]
    );
    let led = static_init!(
        capsules::led::LED<'static>,
        capsules::led::LED::new(&led_pins[..])
    );

    // BUTTONs
    // let button = components::button::ButtonComponent::new(board_kernel).finalize(
    //     components::button_component_helper!((
    //         stm32f4xx::gpio::PinId::PC13.get_pin().as_ref().unwrap(),
    //         capsules::button::GpioMode::LowWhenPressed,
    //         kernel::hil::gpio::FloatingState::PullNone
    //     )),
    // );

    // ALARM

    let mux_alarm = static_init!(
        MuxAlarm<'static, imxrt1050::gpt1::Gpt1>,
        MuxAlarm::new(&imxrt1050::gpt1::GPT1)
    );
    imxrt1050::gpt1::GPT1.set_client(mux_alarm);

    let virtual_alarm = static_init!(
        VirtualMuxAlarm<'static, imxrt1050::gpt1::Gpt1>,
        VirtualMuxAlarm::new(mux_alarm)
    );
    let alarm = static_init!(
        capsules::alarm::AlarmDriver<'static, VirtualMuxAlarm<'static, imxrt1050::gpt1::Gpt1>>,
        capsules::alarm::AlarmDriver::new(
            virtual_alarm,
            board_kernel.create_grant(&memory_allocation_capability)
        )
    );
    virtual_alarm.set_client(alarm);

    // GPIO
    // let gpio = GpioComponent::new(board_kernel).finalize(components::gpio_component_helper!(
    //     // Arduino like RX/TX
        
    // ));

    let imxrt1050 = Imxrt1050EVKB {
        console: console,
        ipc: kernel::ipc::IPC::new(board_kernel, &memory_allocation_capability),
        led: led,
        // button: button,
        alarm: alarm,
        // gpio: gpio,
    };

    // // Optional kernel tests
    // //
    // // See comment in `boards/imix/src/main.rs`
    // virtual_uart_rx_test::run_virtual_uart_receive(mux_uart);

    debug!("Initialization complete. Entering main loop");

    extern "C" {
        /// Beginning of the ROM region containing app images.
        ///
        /// This symbol is defined in the linker script.
        static _sapps: u8;
    }

    // let pin = imxrt1050::gpio::PinId::P1_09.get_pin().as_ref().unwrap();
    // pin.make_output();
    // pin.clear();
    // debug!("Almost loaded!");

    kernel::procs::load_processes(
        board_kernel,
        chip,
        &_sapps as *const u8,
        &mut APP_MEMORY,
        &mut PROCESSES,
        FAULT_RESPONSE,
        &process_management_capability,
    );

    board_kernel.kernel_loop(
        &imxrt1050,
        chip,
        Some(&imxrt1050.ipc),
        &main_loop_capability,
    );
}
