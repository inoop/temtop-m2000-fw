# Temtop M2000 Hardware Reference

Revision 1.0

This document describes the electrical architecture of the Temtop M2000
air quality monitor main board, intended for firmware developers. It covers
the microcontroller, memory map, power control, sensor interfaces, display,
user inputs and connectors.

---

## 1. Overview

The M2000 is a handheld, battery-powered air quality monitor measuring
particulate matter (PM2.5, PM10, particle count), carbon dioxide (CO₂),
formaldehyde (HCHO), temperature and relative humidity. Readings are shown
on a 240×320 colour TFT. The unit charges and communicates over USB.

### 1.1 Block diagram

```
                         +---------------------------+
   PM sensor   <-UART2-->|                           |<--FSMC 16-bit--> ILI9341 TFT
   CO2 sensor  <-UART1-->|                           |--PE2-----------> Backlight
   T/RH sensor <--I2C--->|      STM32F103VET6        |--PB15 (PWM)----> Buzzer
   HCHO cell   ---ADC--->|      Cortex-M3, 72 MHz    |<--PA0,PA1,PC1-3-- Keys
   Battery     ---ADC--->|                           |<-PA11/PA12------> USB FS
                         |                           |--PC0-----------> Power latch
   32.768 kHz ---------->|  RTC (battery-backed)     |<--PE3----------- VBUS detect
   8 MHz ---------------->|                           |
                         +---------------------------+
```

### 1.2 Main components

| Function | Part |
|---|---|
| MCU | STMicroelectronics STM32F103VET6, LQFP100 |
| Display | TFT, ILI9341 controller, 240×320, RGB565 |
| PM sensor | Laser scattering module, UART, "JC" framed protocol |
| CO₂ sensor | NDIR module, UART, MH-Z19 command set |
| HCHO sensor | Electrochemical cell with analog front end |
| T/RH sensor | SHT2x / HTU21-compatible, I²C address 0x40 |
| Charger | CN3153 single-cell Li-ion linear charger |
| Battery | Single-cell Li-ion, 3.7 V nominal |

---

## 2. Microcontroller

| Parameter | Value |
|---|---|
| Device | STM32F103VET6 (high density) |
| Core | ARM Cortex-M3 |
| Flash | 512 KB |
| SRAM | 64 KB at 0x2000 0000 |
| Package | LQFP100 |

### 2.1 Clock tree

| Clock | Source | Frequency |
|---|---|---|
| HSE | Crystal | 8 MHz |
| SYSCLK | PLL (HSE × 9) | 72 MHz |
| AHB | SYSCLK | 72 MHz |
| APB2 | AHB / 1 | 72 MHz |
| APB1 | AHB / 2 | 36 MHz |
| USB | PLL / 1.5 | 48 MHz |
| ADC | APB2 / 8 | 9 MHz |
| LSE | Crystal | 32.768 kHz (RTC) |

Flash must be configured for two wait states with prefetch enabled before
switching SYSCLK to the PLL.

---

## 3. Memory map

### 3.1 Internal flash

| Address range | Size | Contents |
|---|---|---|
| 0x0800 0000 – 0x0800 4FFF | 20 KB | USB DFU bootloader |
| 0x0800 5000 – 0x0807 F7FF | 490 KB | Application |
| 0x0807 F800 – 0x0807 FFFF | 2 KB | Settings page (contains the calibration block) |

The application image is linked at 0x0800 5000. Its first word must be a
valid initial stack pointer in SRAM; the bootloader checks this before
starting the application. The application must set `SCB->VTOR` to
0x0800 5000.

### 3.2 Calibration block

A 68-byte calibration block is stored at **0x0807 FC00**, inside the last
2 KB flash page. It is written at the factory and holds sensor
scaling factors, alarm thresholds and the HCHO response curve. Its layout
is described in [sensor-processing.md](sensor-processing.md#1-calibration-block).
Because flash is erased in 2 KB pages, updates must read the full page at
0x0807 F800, erase it and reprogram it with the block merged in.

### 3.3 Backup registers

The RTC domain is powered from the battery and keeps its contents while
the unit is off.

| Register | Use |
|---|---|
| BKP_DR1 | 0xA5A5 when the RTC has been configured |
| BKP_DR2 | User interface preferences (application defined) |

---

## 4. Power subsystem

### 4.1 Power latch

The main supply is held by a latch controlled by **PC0** together with the
power key (PA0). Pressing the power key powers the MCU; the firmware must
then take over the latch within a few hundred milliseconds.

Required power-on sequence:

1. Configure PC0, PE2, PB0 and PA8 as push-pull outputs and drive them HIGH.
2. Drive PB15 (buzzer) LOW.
3. Wait approximately 200 ms.
4. Configure PB1 as a push-pull output and drive it LOW. Wait up to 300 ms
   for the power key (PA0) to be released; if it is still held after
   300 ms, drive PB1 HIGH and continue.
5. Drive PE2 HIGH and **PC0 LOW**. The unit now stays powered after the
   key is released.

Power-off sequence:

1. Drive PC0 HIGH.
2. Wait until PA0 reads LOW on two samples 20 ms apart.
3. Drive PB1 LOW. The supply collapses shortly afterwards.

> **Note.** A system reset (for example `SYSRESETREQ`) returns all GPIOs to
> their reset state, which releases the latch and removes power. The unit
> also switches off after leaving DFU mode. Press the power key to restart.

### 4.2 Sensor power

| Pin | Active | Supplies |
|---|---|---|
| PB0 | HIGH | CO₂ sensor |
| PB1 | HIGH | PM sensor (also part of the power-on handshake) |
| PA8 | HIGH | HCHO analog front end |

### 4.3 USB power detect

**PE3** reads HIGH while USB power is present. Configure it as an input
with pull-down.

### 4.4 Battery

The battery is charged from USB by a CN3153 linear charger. Battery
voltage is measured on **PC4 (ADC1 channel 14)** through a resistor divider.
The standard battery gauge maps ADC counts to an 11-segment indicator:

```
segments = clamp( (adc - 2251) / 293 * 11 , 0 , 11 )
```

where `adc` is the mean of 10 conversions. The displayed value is
averaged over 10 consecutive one-second readings.

---

## 5. Sensor interfaces

### 5.1 Particulate matter sensor — USART2

| Parameter | Value |
|---|---|
| Pins | PA2 TX (AF push-pull), PA3 RX (floating input) |
| Format | 115200 baud, 8N1 |
| Power | PB1 |

The sensor transmits one 20-byte frame per measurement without being
polled. DMA1 channel 6 is recommended for reception into a circular
buffer.

```
Offset  Size  Content
0       2     'J' 'C' (0x4A 0x43)
2       4     PM2.5, µg/m³, IEEE 754 float, little-endian
6       4     PM10,  µg/m³, IEEE 754 float, little-endian
10      8     reserved
18      1     checksum
19      1     0x0D
```

### 5.2 CO₂ sensor — USART1

| Parameter | Value |
|---|---|
| Pins | PA9 TX (AF push-pull), PA10 RX (floating input) |
| Format | 9600 baud, 8N1 |
| Power | PB0 |
| Protocol | MH-Z19 command set |

| Command | Bytes |
|---|---|
| Read concentration | `FF 01 86 00 00 00 00 00 79` |
| Zero point calibration | `FF 01 87 00 00 00 00 00 78` |

Response to the read command: `FF 86 HH LL xx xx xx xx CS`, where
`HH:LL` is the concentration in ppm, big-endian. The sensor does not answer
for approximately two minutes after power is applied.

Zero point calibration sets the current concentration as 400 ppm. Run it
only after the unit has been in fresh outdoor air for at least 20 minutes.

### 5.3 Formaldehyde sensor — ADC1 channel 15

| Parameter | Value |
|---|---|
| Pin | PC5, analog input |
| Power | PA8 (analog front end) |
| ADC | ADC1, channel 15, 55.5-cycle sample time |

ADC1 scans channel 14 (battery) and channel 15 (HCHO) continuously. The
results go by DMA1 channel 1 into a 20-entry circular buffer, so even
entries hold channel 14 and odd entries hold channel 15. The front-end
output rises with concentration; conversion to µg/m³ uses the
five-point curve in the calibration block.

### 5.4 Temperature and humidity sensor — I²C

| Parameter | Value |
|---|---|
| Pins | PB8 SCL, PB9 SDA, both open-drain |
| Address | 0x40 |
| Interface | Software I²C, ~100 kHz |

The STM32 I²C peripherals are not used. Both lines must be open-drain,
because the sensor stretches SCL during a measurement (up to 85 ms).

| Command | Function |
|---|---|
| 0xE5 | Measure relative humidity, hold master |
| 0xE0 | Read temperature from the previous humidity measurement |
| 0xFE | Soft reset |

Each read returns two data bytes (MSB first) followed by a CRC byte.

---

## 6. Display

| Parameter | Value |
|---|---|
| Controller | ILI9341 |
| Resolution | 240 × 320, portrait |
| Colour format | RGB565 |
| Interface | FSMC bank 1, NOR/SRAM mode, 16-bit, write-only |
| Command register | 0x6000 0000 |
| Data register | 0x6002 0000 (A16 = D/C) |
| Reset | PE1, active LOW |
| Backlight | PE2, HIGH = on |

FSMC timing registers: `BCR1 = 0x0000 1059`, `BTR1 = 0x1000 0101`.

The read strobe is not connected, so the panel cannot be read back. After
the controller is initialised, set MADCTL (0x36) to 0x08 (BGR, no
mirroring) for the correct orientation.

### 6.1 FSMC pin assignment

| Signal | Pin |
|---|---|
| D0, D1 | PD14, PD15 |
| D2, D3 | PD0, PD1 |
| D4 – D12 | PE7 – PE15 |
| D13 – D15 | PD8 – PD10 |
| NOE | PD4 |
| NWE | PD5 |
| NE1 (CS) | PD7 |
| A16 (D/C) | PD11 |

---

## 7. User interface

### 7.1 Keys

All keys connect to ground when pressed. Configure them as inputs with
pull-up.

| Key | Pin | Notes |
|---|---|---|
| POWER | PA0 | Power on / off (hold) |
| MENU | PA1 | Held at power-on: enter DFU mode |
| OK | PC1 | |
| UP | PC2 | |
| DOWN | PC3 | |

### 7.2 Buzzer

The buzzer is driven by **TIM1 channel 3N on PB15** (AF push-pull).

| Register | Value | Result |
|---|---|---|
| PSC | 71 | 1 MHz timer clock |
| ARR | 330 | 3.02 kHz tone |
| CCR3 | 50 | 15 % duty |

Keep the output in forced-inactive mode (OC3M = 100) while silent, so PB15
rests LOW. TIM1 requires MOE in BDTR to be set.

---

## 8. USB

| Parameter | Value |
|---|---|
| Speed | Full speed (12 Mbit/s) |
| Pins | PA11 D−, PA12 D+ |
| D+ pull-up | 1.5 kΩ, fixed |

Because the D+ pull-up is fixed, the host only notices a restart if the
firmware drives PA12 LOW for about 10 ms before enabling the USB
peripheral.

### 8.1 DFU bootloader

The bootloader occupies the first 20 KB of flash and implements the
STMicroelectronics DfuSe protocol (VID 0x0483, PID 0xDF11), compatible with
`dfu-util`.

To enter DFU mode, hold **MENU** while switching the unit on. Otherwise the
bootloader starts the application at 0x0800 5000.

DfuSe alternate setting 0, "Internal Flash", exposes the full 512 KB. The
first 12 KB are read-only; the remainder can be read, erased and written.

---

## 9. Real-time clock

The STM32 RTC runs from the 32.768 kHz LSE crystal with a prescaler of
32768 (1 Hz). It is powered from the battery, so the counter keeps running
while the unit is switched off.

---

## 10. Connectors

### 10.1 USB

USB receptacle, used for charging and data. See section 8.

### 10.2 SW01 — programming header

4-pin header on the main board, pin 1 at the top.

| Pin | Signal |
|---|---|
| 1 | + (3.3 V) |
| 2 | − (GND) |
| 3 | DIO (SWDIO) |
| 4 | CLK (SWCLK) |

---

## 11. Pin assignment summary

### Port A

| Pin | Mode | Function |
|---|---|---|
| PA0 | Input, pull-up | POWER key |
| PA1 | Input, pull-up | MENU key / DFU select |
| PA2 | AF push-pull | USART2 TX (PM sensor) |
| PA3 | Floating input | USART2 RX (PM sensor) |
| PA8 | Push-pull output | HCHO front-end power |
| PA9 | AF push-pull | USART1 TX (CO₂ sensor) |
| PA10 | Floating input | USART1 RX (CO₂ sensor) |
| PA11 | USB | USB D− |
| PA12 | USB | USB D+ |
| PA13 | SWD | SWDIO |
| PA14 | SWD | SWCLK |

### Port B

| Pin | Mode | Function |
|---|---|---|
| PB0 | Push-pull output | CO₂ sensor power |
| PB1 | Push-pull output | PM sensor power |
| PB8 | Open-drain output | I²C SCL (T/RH sensor) |
| PB9 | Open-drain output | I²C SDA (T/RH sensor) |
| PB15 | AF push-pull | TIM1_CH3N buzzer |

### Port C

| Pin | Mode | Function |
|---|---|---|
| PC0 | Push-pull output | Power latch (LOW = hold) |
| PC1 | Input, pull-up | OK key |
| PC2 | Input, pull-up | UP key |
| PC3 | Input, pull-up | DOWN key |
| PC4 | Analog | ADC1 CH14, battery voltage |
| PC5 | Analog | ADC1 CH15, HCHO sensor |

### Port D

| Pin | Mode | Function |
|---|---|---|
| PD0, PD1 | AF push-pull | FSMC D2, D3 |
| PD4 | AF push-pull | FSMC NOE |
| PD5 | AF push-pull | FSMC NWE |
| PD7 | AF push-pull | FSMC NE1 (LCD CS) |
| PD8 – PD10 | AF push-pull | FSMC D13 – D15 |
| PD11 | AF push-pull | FSMC A16 (LCD D/C) |
| PD14, PD15 | AF push-pull | FSMC D0, D1 |

### Port E

| Pin | Mode | Function |
|---|---|---|
| PE1 | Push-pull output | LCD reset |
| PE2 | Push-pull output | LCD backlight |
| PE3 | Input, pull-down | USB VBUS detect |
| PE7 – PE15 | AF push-pull | FSMC D4 – D12 |

### Peripheral allocation

| Peripheral | Use |
|---|---|
| USART1 | CO₂ sensor |
| USART2 | PM sensor |
| ADC1 | Battery (CH14), HCHO (CH15) |
| DMA1 CH1 | ADC1 |
| DMA1 CH6 | USART2 RX |
| TIM1 | Buzzer PWM |
| FSMC bank 1 | Display |
| USB | USB device |
| RTC | Clock |
