// USB CDC serial. VID:PID 16c0:27dd is the shared V-USB pair for CDC-ACM
// devices, so hosts identify the unit by its strings. The device and port live in statics so delay_ms() can
// keep polling them from anywhere.

use stm32_usbd::UsbBus;
use usb_device::bus::UsbBusAllocator;
use usb_device::prelude::*;
use usbd_serial::SerialPort;

use crate::hw::{self, pins::USB_DP};

struct UsbPeriph;
unsafe impl Sync for UsbPeriph {}
unsafe impl Send for UsbPeriph {}

unsafe impl stm32_usbd::UsbPeripheral for UsbPeriph {
    const REGISTERS: *const () = 0x4000_5C00 as *const ();
    const DP_PULL_UP_FEATURE: bool = false;
    const EP_MEMORY: *const () = 0x4000_6000 as *const ();
    const EP_MEMORY_SIZE: usize = 512;
    const EP_MEMORY_ACCESS_2X16: bool = false;

    fn enable() {
        hw::rcc_enable(hw::RCC_APB1ENR, hw::apb1::USB);
    }

    fn startup_delay() {
        cortex_m::asm::delay(72);
    }
}

type UsbBusType = UsbBus<UsbPeriph>;

static mut SERIAL: *mut SerialPort<'static, UsbBusType> = core::ptr::null_mut();
static mut DEVICE: *mut UsbDevice<'static, UsbBusType> = core::ptr::null_mut();

/// Hold the USB peripheral in reset and pull D+ low for 10 ms so the host
/// sees a fresh device.
pub fn force_reenumerate() {
    hw::write(0x4000_5C40, 0x0003); // USB_CNTR: FRES | PDWN
    USB_DP.config(hw::OUT_PP);
    USB_DP.clr();
    crate::clock::delay_ms(10);
    USB_DP.config(hw::IN_FLOAT);
}

/// 96-bit factory unique ID as 24 hex digits
fn serial_number() -> &'static str {
    static mut BUF: [u8; 24] = [0; 24];
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(BUF) };
    for (i, b) in buf.iter_mut().enumerate() {
        let byte = unsafe { core::ptr::read_volatile((0x1FFF_F7E8 + i / 2) as *const u8) };
        let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0xF };
        *b = b"0123456789ABCDEF"[nibble as usize];
    }
    core::str::from_utf8(buf).unwrap_or("0")
}

pub fn init() {
    let bus: &'static UsbBusAllocator<UsbBusType> =
        cortex_m::singleton!(: UsbBusAllocator<UsbBusType> = UsbBusType::new(UsbPeriph)).unwrap();
    let serial = cortex_m::singleton!(: SerialPort<'static, UsbBusType> = SerialPort::new(bus)).unwrap();
    let device = cortex_m::singleton!(: UsbDevice<'static, UsbBusType> =
        UsbDeviceBuilder::new(bus, UsbVidPid(0x16c0, 0x27dd))
            .strings(&[StringDescriptors::default()
                .manufacturer("Temtop M2000 open firmware")
                .product("Air quality monitor")
                .serial_number(serial_number())])
            .unwrap()
            .device_class(usbd_serial::USB_CLASS_CDC)
            .build()
    ).unwrap();
    unsafe {
        SERIAL = serial;
        DEVICE = device;
    }
}

pub fn poll() {
    unsafe {
        if !SERIAL.is_null() && !DEVICE.is_null() {
            (*DEVICE).poll(&mut [&mut *SERIAL]);
        }
    }
}

pub fn write(data: &[u8]) {
    unsafe {
        if SERIAL.is_null() { return; }
        let serial = &mut *SERIAL;
        let mut offset = 0;
        while offset < data.len() {
            match serial.write(&data[offset..]) {
                Ok(n) => offset += n,
                Err(_) => break,
            }
        }
    }
}
