/**
 * Minimal telemetry + dispatch client for the GW-01 emulator.
 *
 * Subscribes to a few MQTT topics, then writes a setpoint over the REST
 * control API and watches the plant obey.
 *
 * Setup:   npm install        (installs mqtt)
 * Run:     npx tsx read_and_dispatch.ts
 * Needs the emulator plus an MQTT broker, e.g. `docker compose up` in the
 * repository root. Topic reference: refmodel/gw01-signal-map.csv.
 */

import mqtt from "mqtt";

const MQTT_URL = "mqtt://127.0.0.1:1883";
const REST_URL = "http://127.0.0.1:8080";

interface Sample {
  ts: number;
  value: number;
  unit: string;
}

async function setSetpoint(watts: number): Promise<void> {
  const res = await fetch(`${REST_URL}/api/v1/setpoint`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ watts }),
  });
  if (!res.ok) throw new Error(`setpoint rejected: ${res.status}`);
  console.log(`setpoint written: ${(watts / 1e6).toFixed(1)} MW`);
}

async function backToPlan(): Promise<void> {
  await fetch(`${REST_URL}/api/v1/setpoint`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ mode: "plan" }),
  });
  console.log("control handed back to the internal plan");
}

const client = mqtt.connect(MQTT_URL);

client.on("connect", async () => {
  console.log(`connected to ${MQTT_URL}`);
  client.subscribe([
    "bess/gw01/site/poi/active_power_w",
    "bess/gw01/site/soc_pct",
    "bess/gw01/site/poi/frequency_hz",
  ]);
  // Discharge at 40 MW for a while, then hand control back.
  await setSetpoint(40e6);
  setTimeout(async () => {
    await backToPlan();
    setTimeout(() => {
      client.end();
      process.exit(0);
    }, 3000);
  }, 15000);
});

client.on("message", (topic: string, payload: Buffer) => {
  const sample = JSON.parse(payload.toString()) as Sample;
  console.log(`${topic} = ${sample.value.toFixed(2)} ${sample.unit}`);
});
