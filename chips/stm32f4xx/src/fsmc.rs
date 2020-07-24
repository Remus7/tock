use crate::rcc;
use core::cell::Cell;
use kernel::common::cells::{OptionalCell, TakeCell};
use kernel::common::deferred_call::DeferredCall;
use kernel::common::registers::{register_bitfields, ReadWrite};
use kernel::common::StaticRef;
use kernel::debug;
use kernel::hil::bus::{Bus, BusWidth, Client};
use kernel::{ClockInterface, ReturnCode};

use crate::deferred_calls::DeferredCallTask;

/// FSMC peripheral interface
#[repr(C)]
struct FsmcBankRegisters {
    /// SRAM/NOR-Flash chip-select control register
    bcr1: ReadWrite<u32, BCR::Register>,
    /// SRAM/NOR-Flash chip-select timing register
    btr1: ReadWrite<u32, BTR::Register>,
    /// SRAM/NOR-Flash chip-select control register
    bcr2: ReadWrite<u32, BCR::Register>,
    /// SRAM/NOR-Flash chip-select timing register
    btr2: ReadWrite<u32, BTR::Register>,
    /// SRAM/NOR-Flash chip-select control register
    bcr3: ReadWrite<u32, BCR::Register>,
    /// SRAM/NOR-Flash chip-select timing register
    btr3: ReadWrite<u32, BTR::Register>,
    /// SRAM/NOR-Flash chip-select control register
    bcr4: ReadWrite<u32, BCR::Register>,
    /// SRAM/NOR-Flash chip-select timing register
    btr4: ReadWrite<u32, BTR::Register>,
    _reseved: [u8; 228],
    /// SRAM/NOR-Flash write timing registers
    bwtr1: ReadWrite<u32, BWTR::Register>,
    /// SRAM/NOR-Flash write timing registers
    bwtr2: ReadWrite<u32, BWTR::Register>,
    /// SRAM/NOR-Flash write timing registers
    bwtr3: ReadWrite<u32, BWTR::Register>,
    /// SRAM/NOR-Flash write timing registers
    bwtr4: ReadWrite<u32, BWTR::Register>,
}

register_bitfields![u32,
    BCR [
        /// Write FIFO Disable
        WFDIS OFFSET(21) NUMBITS(1) [],
        /// Continuous Clock Enable
        CCLKEN OFFSET(20) NUMBITS(1) [],
        /// Write burst enable
        CBURSTRW OFFSET(19) NUMBITS(1) [],
        /// CRAM page size
        CPSIZE OFFSET(16) NUMBITS(3) [
            NO_BURST = 0b000,
            BYTES_128 = 0b001,
            BYTES_256 = 0b010,
            BYTES_1024 = 0b011
        ],
        /// Wait signal during asynchronous transfers
        ASYNCWAIT OFFSET(15) NUMBITS(1) [],
        /// Extended mode enable
        EXTMOD OFFSET(14) NUMBITS(1) [],
        /// Wait enable bit
        WAITEN OFFSET(13) NUMBITS(1) [],
        /// Write enable bit
        WREN OFFSET(12) NUMBITS(1) [],
        /// Wait timing configuration
        WAITCFG OFFSET(11) NUMBITS(1) [],
        /// Wait signal polarity bit
        WAITPOL OFFSET(9) NUMBITS(1) [],
        /// Burst enable bit
        BURSTEN OFFSET(8) NUMBITS(1) [],
        /// Flash access enable
        /// Enables NOR Flash memory access operations
        FACCEN OFFSET(6) NUMBITS(1) [],
        /// Memory data bus_width width
        MWID OFFSET(4) NUMBITS(2) [
            BITS_8 = 0b00,
            BITS_16 = 0b01
        ],
        /// Memory type
        MTYP OFFSET(2) NUMBITS(2) [
            SRAM = 0b00,
            PSRAM = 0b01,
            NOR = 0b10
        ],
        /// Address/data multiplexing enable bit
        MUXEN OFFSET(1) NUMBITS(1) [],
        /// Memory bank enable bit
        MBKEN OFFSET(0) NUMBITS(1) []
    ],
    BTR [
        /// Access mode
        ACCMOD OFFSET(28) NUMBITS(2) [
            A = 0b00,
            B = 0b01,
            C = 0b10,
            D = 0b11
        ],
        /// Data latency for synchronous memory
        DATLAT OFFSET(24) NUMBITS(4) [],
        /// Clock divide ratio (for FSMC_CLK signal)
        CLKDIV OFFSET(20) NUMBITS(4) [],
        /// Bus turnaround phase duration
        BUSTURN OFFSET(16) NUMBITS(4) [],
        /// Data-phase duration
        DATAST OFFSET(8) NUMBITS(8) [],
        /// Address-hold phase duration
        ADDHLD OFFSET(4) NUMBITS(4) [],
        /// Address setup phase duration
        ADDSET OFFSET(0) NUMBITS(4) []
    ],
    BWTR [
        /// Access mode
        ACCMOD OFFSET(28) NUMBITS(2) [
            A = 0b00,
            B = 0b01,
            C = 0b10,
            D = 0b11
        ],
        BUSTURN OFFSET(16) NUMBITS(4) [],
        /// Data-phase duration
        DATAST OFFSET(8) NUMBITS(8) [],
        /// Address-hold phase duration
        ADDHLD OFFSET(4) NUMBITS(4) [],
        /// Address setup phase duration
        ADDSET OFFSET(0) NUMBITS(4) []
    ]
];

/// This mechanism allows us to schedule "interrupts" even if the hardware
/// does not support them.
static DEFERRED_CALL: DeferredCall<DeferredCallTask> =
    unsafe { DeferredCall::new(DeferredCallTask::Fsmc) };

const FSMC_BASE: StaticRef<FsmcBankRegisters> =
    unsafe { StaticRef::new(0xA000_0000 as *const FsmcBankRegisters) };

/// FSMC Bank
#[repr(C)]
struct FsmcBank {
    /// Address
    reg: ReadWrite<u16>,
    /// Data
    ram: ReadWrite<u16>,
}

fn bus_width_in_bytes(bus_width: &BusWidth) -> usize {
    match bus_width {
        BusWidth::Bits8 => 1,
        BusWidth::Bits16BE | BusWidth::Bits16LE => 2,
        BusWidth::Bits32BE | BusWidth::Bits32LE => 3,
        BusWidth::Bits64BE | BusWidth::Bits64LE => 4,
    }
}

const FSMC_BANK1: StaticRef<FsmcBank> = unsafe { StaticRef::new(0x60000000 as *const FsmcBank) };
const FSMC_BANK2_RESERVED: StaticRef<FsmcBank> = unsafe { StaticRef::new(0x0 as *const FsmcBank) };
const FSMC_BANK3: StaticRef<FsmcBank> = unsafe { StaticRef::new(0x68000000 as *const FsmcBank) };
const FSMC_BANK4_RESERVED: StaticRef<FsmcBank> = unsafe { StaticRef::new(0x0 as *const FsmcBank) };

pub struct Fsmc {
    registers: StaticRef<FsmcBankRegisters>,
    bank: [StaticRef<FsmcBank>; 4],
    clock: FsmcClock,

    client: OptionalCell<&'static dyn Client>,

    buffer: TakeCell<'static, [u8]>,
    bus_width: Cell<usize>,
    len: Cell<usize>,
}

impl Fsmc {
    const fn new(
        base_addr: StaticRef<FsmcBankRegisters>,
        bank_addr: [StaticRef<FsmcBank>; 4],
    ) -> Fsmc {
        Fsmc {
            registers: base_addr,
            bank: bank_addr,
            clock: FsmcClock(rcc::PeripheralClock::AHB3(rcc::HCLK3::FSMC)),
            client: OptionalCell::empty(),

            buffer: TakeCell::empty(),
            bus_width: Cell::new(1),
            len: Cell::new(0),
        }
    }

    pub fn enable(&self) {
        self.registers.bcr1.modify(
            BCR::MBKEN::SET
                + BCR::MUXEN::CLEAR
                + BCR::MTYP::SRAM
                + BCR::MWID::BITS_16
                + BCR::BURSTEN::CLEAR
                + BCR::WAITPOL::CLEAR
                + BCR::WAITCFG::CLEAR
                + BCR::WREN::SET
                + BCR::WAITEN::CLEAR
                + BCR::EXTMOD::SET
                + BCR::ASYNCWAIT::CLEAR
                + BCR::CBURSTRW::CLEAR
                + BCR::WFDIS::SET
                + BCR::CPSIZE::NO_BURST
                + BCR::CCLKEN::CLEAR,
        );
        self.registers.btr1.modify(
            BTR::ADDSET.val(9)
                + BTR::ADDHLD.val(1)
                + BTR::DATAST.val(36)
                + BTR::BUSTURN.val(1)
                + BTR::CLKDIV.val(2)
                + BTR::DATLAT.val(2)
                + BTR::ACCMOD::A,
        );
        self.registers.bwtr1.modify(
            BWTR::ADDSET.val(1)
                + BWTR::ADDHLD.val(1)
                + BWTR::DATAST.val(7)
                + BWTR::BUSTURN.val(0)
                + BWTR::ACCMOD::A,
        );
        self.enable_clock();
    }

    pub fn disable(&self) {
        self.disable_clock();
    }

    pub fn enable_clock(&self) {
        self.clock.enable();
    }

    pub fn disable_clock(&self) {
        self.clock.disable();
    }

    pub fn handle_interrupt(&self) {
        self.buffer.take().map(|buffer| {
            self.client.map(move |client| {
                client.command_complete(buffer, self.len.get());
            });
        });
    }

    // pub fn write(&self, addr: u16, data: u16) {
    //     self.bank[0].reg.set(addr);
    //     unsafe {
    //         llvm_asm!("dsb 0xf");
    //     }
    //     self.bank[0].ram.set(data);
    //     unsafe {
    //         llvm_asm!("dsb 0xf");
    //     }
    // }

    pub fn read_reg(&self, addr: u16) -> u16 {
        self.bank[0].reg.set(addr);
        unsafe {
            llvm_asm!("dsb 0xf");
        }
        self.bank[0].ram.get()
    }

    #[inline]
    fn write_reg(&self, addr: u16) {
        self.bank[0].reg.set(addr);
        unsafe {
            llvm_asm!("dsb 0xf");
        }
    }

    #[inline]
    fn write_data(&self, data: u16) {
        self.bank[0].ram.set(data);
        unsafe {
            llvm_asm!("dsb 0xf");
        }
    }
}

struct FsmcClock(rcc::PeripheralClock);

impl ClockInterface for FsmcClock {
    fn is_enabled(&self) -> bool {
        self.0.is_enabled()
    }

    fn enable(&self) {
        self.0.enable();
    }

    fn disable(&self) {
        self.0.disable();
    }
}

impl Bus for Fsmc {
    fn write_addr(
        &self,
        addr_width: BusWidth,
        addr: usize,
        data_width: BusWidth,
        buffer: &'static mut [u8],
        len: usize,
    ) -> Result<(), (ReturnCode, &'static mut [u8])> {
        debug!("write reg {} len {}", addr, len);
        match addr_width {
            BusWidth::Bits8 | BusWidth::Bits16BE | BusWidth::Bits16LE => match data_width {
                BusWidth::Bits8 | BusWidth::Bits16LE | BusWidth::Bits16BE => {
                    self.write_reg(addr as u16);
                    self.write(data_width, buffer, len)
                }
                _ => Err((ReturnCode::ENOSUPPORT, buffer)),
            },
            _ => Err((ReturnCode::ENOSUPPORT, buffer)),
        }
    }
    fn read_addr(
        &self,
        _addr_width: BusWidth,
        _addr: usize,
        _data_width: BusWidth,
        buffer: &'static mut [u8],
        _len: usize,
    ) -> Result<(), (ReturnCode, &'static mut [u8])> {
        Err((ReturnCode::ENOSUPPORT, buffer))
    }

    fn write(
        &self,
        data_width: BusWidth,
        buffer: &'static mut [u8],
        len: usize,
    ) -> Result<(), (ReturnCode, &'static mut [u8])> {
        debug!("write {}", len);
        match data_width {
            BusWidth::Bits8 | BusWidth::Bits16BE | BusWidth::Bits16LE => {
                let bytes = bus_width_in_bytes(&data_width);
                if len > 0 {
                    debug!("{:?}", &buffer[0..4]);
                }
                if buffer.len() >= len * bytes {
                    for pos in 0..len {
                        let mut data: u16 = 0;
                        for byte in 0..bytes {
                            data = data
                                | (buffer[bytes * pos
                                    + match data_width {
                                        BusWidth::Bits8 | BusWidth::Bits16LE => byte,
                                        BusWidth::Bits16BE => (bytes - byte - 1),
                                        _ => panic!("fsmc bus error"),
                                    }] as u16)
                                    << (8 * byte);
                        }
                        self.write_data(data);
                    }
                    self.buffer.replace(buffer);
                    self.bus_width.set(bytes);
                    self.len.set(len);
                    DEFERRED_CALL.set();
                    Ok(())
                } else {
                    Err((ReturnCode::ENOMEM, buffer))
                }
            }
            _ => Err((ReturnCode::ENOSUPPORT, buffer)),
        }
    }

    fn read(
        &self,
        _data_width: BusWidth,
        buffer: &'static mut [u8],
        _len: usize,
    ) -> Result<(), (ReturnCode, &'static mut [u8])> {
        Err((ReturnCode::ENOSUPPORT, buffer))
    }

    fn set_client(&self, client: &'static dyn Client) {
        self.client.replace(client);
    }
}

pub static mut FSMC: Fsmc = Fsmc::new(
    FSMC_BASE,
    [
        FSMC_BANK1,
        FSMC_BANK2_RESERVED,
        FSMC_BANK3,
        FSMC_BANK4_RESERVED,
    ],
);
