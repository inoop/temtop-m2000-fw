# Temtop M2000 Sensor Data Processing

This document specifies how raw sensor data is turned into displayed
values. It follows the original M2000 firmware exactly, so a unit gives
the same readings with either firmware.

Conventions: arithmetic is IEEE 754 single precision (`f32`) unless marked
`f64`. "Truncate" means conversion to an unsigned integer, rounding toward
zero.

---

## 1. Calibration block

68 bytes at flash address **0x0807 FC00**. The block is valid when bytes
0–2 are `01 03 01`. If it is not valid, the defaults below apply.

| Offset | Type | Default | Meaning |
|---|---|---|---|
| 0x00 | u8 × 3 | 01 03 01 | Signature |
| 0x03 | u8 | – | Reserved |
| 0x04 | f32 | 1.0 | PM factor (PM2.5, PM10 and particle count) |
| 0x08 | u32 | – | Reserved |
| 0x0C | f32 | 1.0 | HCHO factor |
| 0x10 | u8 × 3 | – | Reserved |
| 0x13 | u8 | 0 | Temperature unit: 1 = °C, any other value = °F |
| 0x14 | f32 | 1.0 | CO₂ factor |
| 0x18 | u32 | – | Reserved |
| 0x1C | u32 | 100 | PM2.5 alarm threshold, µg/m³ |
| 0x20 | u32 | 80 | HCHO alarm threshold, µg/m³ |
| 0x24 | u32 | 100 | PM10 alarm threshold, µg/m³ |
| 0x28 | u32 | 1000 | CO₂ alarm threshold, ppm |
| 0x2C | u32 | 100 | Reserved threshold |
| 0x30 | u16 × 5 | 1526, 1636, 1771, 2303, 4096 | HCHO curve, ADC counts |
| 0x3A | u16 × 5 | 0, 90, 230, 839, 5000 | HCHO curve, µg/m³ |

Every unit is calibrated individually at the factory. Keep a copy of the
block before experimenting with firmware.

---

## 2. Particulate matter

**Source**: one 20-byte frame per measurement from the PM sensor (see the
hardware reference, section 5.1). A frame is accepted when it starts with
`J C` and byte 19 is `0x0D`.

For each frame:

1. Store PM2.5 and PM10 in two 10-entry rings. Store `PM2.5 × 1450.0` in a
   third ring; this is the particle count, which is derived from PM2.5
   rather than reported by the sensor.
2. Average each ring. Until a ring has filled once, average only the
   entries written so far; after that, average all 10. Sum in index order.
3. Multiply each average by the PM factor (offset 0x04).
4. Clamp the results:
   - PM2.5 and PM10: if `(f64)v < 999.9` keep `v`, else `999.9`; then
     negative values become 0.
   - Particle count: if `v < 1449855.0` keep `v`, else `1449855.0`; then
     negative values become 0.

**Display**

| Value | Rule | Example |
|---|---|---|
| PM2.5, PM10 | values ≥ 1000 become 999.9; `truncate(v × 100) / 10` tenths, one decimal | `18.4` µg/m³ |
| Particle count | `truncate(v × 100) / 100` | `26680` per litre |

**Alarm**: `(i32)PM2.5 > threshold` at offset 0x1C.

---

## 3. Formaldehyde (HCHO)

**Source**: ADC1 channel 15, the mean of 10 conversions (integer,
truncating). Updated every 1.972 s.

1. **Response curve.** With breakpoints `a[0..4]` (offset 0x30) and
   `c[0..4]` (offset 0x3A):
   - if `adc < a[0]`, use `a[0]`
   - for each segment `a[i] ≤ adc < a[i+1]`:
     `r = truncate( (adc − a[i]) × (c[i+1] − c[i]) / (a[i+1] − a[i]) + c[i] )`
   - if `adc ≥ a[4]`, `r = c[4]` (no extrapolation)
2. **Smoothing.** Store `r` in a 3-entry ring and average it, with the same
   fill rule as PM.
3. **Scaling.** Multiply by the HCHO factor (offset 0x0C) to get µg/m³.
4. **Display.** Values ≥ 5000 become 5000, then truncate. The result is
   shown in mg/m³ with three decimals, for example `0.012`.

**Alarm**: `(i32)HCHO > threshold` at offset 0x20, in µg/m³.

### 3.1 Zero calibration

Run zero calibration in clean air.

1. Once per second, store the channel 15 average in a 30-entry ring.
2. After 600 seconds, compute `avg = sum / 30`.
3. Set `d = avg − a[0]`, then `a[0] = avg` and `a[1..4] += d`. All values
   are u16.
4. Write the calibration block back to flash.

Progress is reported as `seconds / 6` percent.

---

## 4. Carbon dioxide (CO₂)

**Source**: the CO₂ sensor is polled every 0.986 s with the read command.
A response is accepted when it starts with `FF 86`.

1. `ppm = (HH << 8) | LL`
2. Store `ppm` in a 10-entry ring and average it, with the same fill rule as PM.
3. Multiply by the CO₂ factor (offset 0x14).
4. **Display.** Values ≥ 5000 become 5000, then `truncate(v × 100) / 100`
   ppm.

**Warm-up**: CO₂ is not shown until 187 s after power-on.

**Alarm**: `(i32)CO₂ > threshold` at offset 0x28.

### 4.1 Zero calibration

User-started. After 1800 s, the zero point command is sent to the sensor.
Progress is reported as `seconds / 18` percent.

---

## 5. Temperature and humidity

Read once per second: humidity first (command 0xE5), then the temperature
belonging to that measurement (command 0xE0).

**Humidity**

```
rh = (raw × 125) / 65536.0 − 6.0
clamp to 0 … 90
display: truncate(rh)                      %
```

**Temperature** (inner arithmetic in `f64`)

```
t = (f32)( raw × 175.72 / 65536.0 − 46.83 )
t ≤ 1      →  0
1 < t < 85 →  t − 1.0        (self-heating compensation)
t ≥ 85     →  85
```

Display, using the unit at offset 0x13:

- °C: `truncate(t)`
- °F: `truncate((f32)(t × 1.8 + 32.0))`, with the inner arithmetic in `f64`

Temperature and humidity are not filtered.

---

## 6. Battery gauge

Once per second, from the ADC1 channel 14 mean of 10 conversions:

```
x    = (adc − 2251.0) / 293.0, minimum 0
bars = min(11, truncate(x × 11.0))
shown = integer mean of the last 10 values of bars
```

---

## 7. Differences in the open firmware

| Item | Original firmware | Open firmware |
|---|---|---|
| PM2.5 ≤ 0.1 µg/m³ | replaced by a random value between 0.1 and 2.0 | actual value shown |
| T/RH refresh | only while the all-readings screen is visible | continuous, 1 Hz |
| Invalid calibration block | defaults written to flash | defaults used in RAM only |
| Low-battery cut-off | yes | no |
| USB output | none | JSON stream, see README |
