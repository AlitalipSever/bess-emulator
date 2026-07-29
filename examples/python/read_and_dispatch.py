#!/usr/bin/env python3
"""Minimal Modbus client for the GW-01 emulator.

Reads telemetry, writes a charge setpoint, watches the plant obey, then
hands control back to the internal dispatch plan.

Requires: pip install "pymodbus>=3.6"
Run the emulator first: cargo run -p bess-emulator   (Modbus on 127.0.0.1:1502)
The full register reference lives in refmodel/gw01-signal-map.csv.
"""

import time

from pymodbus.client import ModbusTcpClient

HOST, PORT = "127.0.0.1", 1502

# Input registers (read-only), from the signal map.
REG_POI_POWER = 0  # i32, W, positive = export
REG_FREQUENCY = 5  # u16, Hz x1000
REG_SITE_SOC = 6  # u16, % x100
REG_SETPOINT = 18  # i32, W

# Holding registers (control surface).
HOLD_SETPOINT = 0  # i32, W; writing switches the EMS to external mode
HOLD_MODE = 2  # u16; 0 = follow internal plan, 1 = external


def to_i32(hi: int, lo: int) -> int:
    raw = (hi << 16) | lo
    return raw - (1 << 32) if raw >= (1 << 31) else raw


def from_i32(value: int) -> list[int]:
    raw = value & 0xFFFFFFFF
    return [(raw >> 16) & 0xFFFF, raw & 0xFFFF]


def read_status(client: ModbusTcpClient) -> None:
    rr = client.read_input_registers(REG_POI_POWER, count=20, slave=1)
    regs = rr.registers
    poi_w = to_i32(regs[REG_POI_POWER], regs[REG_POI_POWER + 1])
    freq = regs[REG_FREQUENCY] / 1000.0
    soc = regs[REG_SITE_SOC] / 100.0
    setpoint_w = to_i32(regs[REG_SETPOINT], regs[REG_SETPOINT + 1])
    print(
        f"POI {poi_w / 1e6:+7.2f} MW | setpoint {setpoint_w / 1e6:+7.2f} MW"
        f" | SoC {soc:5.1f} % | f {freq:.3f} Hz"
    )


def main() -> None:
    client = ModbusTcpClient(HOST, port=PORT)
    if not client.connect():
        raise SystemExit(f"cannot reach the emulator at {HOST}:{PORT}")

    print("-- before")
    read_status(client)

    print("-- writing external setpoint: charge at 30 MW")
    client.write_registers(HOLD_SETPOINT, from_i32(-30_000_000), slave=1)
    for _ in range(5):
        time.sleep(1.0)
        read_status(client)

    print("-- handing control back to the internal plan")
    client.write_register(HOLD_MODE, 0, slave=1)
    time.sleep(1.0)
    read_status(client)

    client.close()


if __name__ == "__main__":
    main()
