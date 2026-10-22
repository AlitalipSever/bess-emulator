//! The slave over real TCP: the closed loop through the control registers,
//! and the alarm registers through the chain.

use std::time::Duration;

use bess_core::alarms::has_bit;
use bess_core::alarms::layout::{block, rack, site};
use std::net::SocketAddr;

use bess_core::PlantConfig;
use bess_scenario::{load, RunParams};
use tokio_modbus::client::{tcp, Context, Reader, Writer};
use tokio_modbus::slave::Slave;

use super::serve;
use crate::fixtures::{self, HOT_BLOCK};
use crate::map::{
    HOLDING_COMMAND_ADDR, HOLDING_COS_PHI_ADDR, HOLDING_COS_PHI_UNDER_ADDR,
    HOLDING_LIMIT_ENABLE_ADDR, HOLDING_LIMIT_PCT_ADDR, HOLDING_LIMIT_REVERT_ADDR,
    HOLDING_MODE_ADDR, HOLDING_Q_MODE_ADDR, HOLDING_Q_SETPOINT_ADDR, HOLDING_SETPOINT_ADDR,
};
use crate::sim;

/// The command line's defaults at full speed.
fn test_params() -> RunParams {
    RunParams {
        seed: 42,
        start_unix_s: 1_767_225_600,
        speed: sim::MAX_SPEED,
        age_years: 0,
    }
}

/// End-to-end closed loop over real TCP: read telemetry, write a
/// charge setpoint, watch the plant obey, hand control back.
#[tokio::test(flavor = "multi_thread")]
async fn modbus_read_write_closed_loop() {
    let addr: SocketAddr = "127.0.0.1:15502".parse().unwrap();
    let (handle, _sim_task) = sim::spawn(test_params(), None, false);
    tokio::spawn(serve(addr, handle));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut ctx = tcp::connect_slave(addr, Slave(1)).await.unwrap();

    // Telemetry: frequency register near 50 Hz (x1000).
    let regs = ctx.read_input_registers(5, 1).await.unwrap().unwrap();
    assert!((49_900..=50_100).contains(&regs[0]), "freq reg {}", regs[0]);

    // Control: write -20 MW (charge) as an i32 pair, high word first.
    let setpoint: i32 = -20_000_000;
    let words = [
        (setpoint as u32 >> 16) as u16,
        (setpoint as u32 & 0xFFFF) as u16,
    ];
    ctx.write_multiple_registers(HOLDING_SETPOINT_ADDR, &words)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The site setpoint register (input 18-19) now reflects the command.
    let sp = ctx.read_input_registers(18, 2).await.unwrap().unwrap();
    let echoed = ((u32::from(sp[0]) << 16) | u32::from(sp[1])) as i32;
    assert_eq!(echoed, setpoint);
    // And the holding bank mirrors external mode.
    let mode = ctx
        .read_holding_registers(HOLDING_MODE_ADDR, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mode[0], 1);

    // Hand control back to the internal plan.
    ctx.write_single_register(HOLDING_MODE_ADDR, 0)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mode = ctx
        .read_holding_registers(HOLDING_MODE_ADDR, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mode[0], 0);

    // Out-of-map access is rejected, not silently served.
    let err = ctx.read_input_registers(60_000, 2).await.unwrap();
    assert!(err.is_err());
}

async fn holding(ctx: &mut Context, addr: u16) -> u16 {
    ctx.read_holding_registers(addr, 1).await.unwrap().unwrap()[0]
}

async fn write(ctx: &mut Context, addr: u16, word: u16) {
    ctx.write_single_register(addr, word)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
}

/// Every holding register of the control surface (M4 phase 2) written
/// over TCP and read back from the state it commands: the limit with its
/// enable and revert time, the reactive mode with its setpoints, and the
/// command register's switchgear and refusals on the event counter.
#[tokio::test(flavor = "multi_thread")]
async fn every_control_register_is_written_end_to_end() {
    let addr: SocketAddr = "127.0.0.1:15505".parse().unwrap();
    let (handle, _sim_task) = sim::spawn(test_params(), None, false);
    tokio::spawn(serve(addr, handle.clone()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut ctx = tcp::connect_slave(addr, Slave(1)).await.unwrap();

    // The limit: 60.0 % enabled with a long revert time (the plant runs
    // at full speed here, thousands of ticks a second, so the timer is
    // read as "still counting", not to the second).
    write(&mut ctx, HOLDING_LIMIT_PCT_ADDR, 600).await;
    write(&mut ctx, HOLDING_LIMIT_REVERT_ADDR, 60_000).await;
    write(&mut ctx, HOLDING_LIMIT_ENABLE_ADDR, 1).await;
    assert_eq!(holding(&mut ctx, HOLDING_LIMIT_PCT_ADDR).await, 600);
    assert_eq!(holding(&mut ctx, HOLDING_LIMIT_ENABLE_ADDR).await, 1);
    let revert = holding(&mut ctx, HOLDING_LIMIT_REVERT_ADDR).await;
    assert!((40_000..=60_000).contains(&revert), "revert {revert}");
    assert!(handle.snapshot.borrow().state.ems.external_limit.enabled);
    assert!(ctx
        .write_single_register(HOLDING_LIMIT_PCT_ADDR, 1001)
        .await
        .unwrap()
        .is_err());

    // The reactive settings: cos φ 0.950 under-excited, then fixed Q of
    // 20 000 kvar, then Q(U).
    write(&mut ctx, HOLDING_COS_PHI_ADDR, 950).await;
    write(&mut ctx, HOLDING_COS_PHI_UNDER_ADDR, 1).await;
    assert_eq!(holding(&mut ctx, HOLDING_COS_PHI_ADDR).await, 950);
    assert_eq!(holding(&mut ctx, HOLDING_COS_PHI_UNDER_ADDR).await, 1);
    let kvar: i32 = 20_000;
    let words = [(kvar as u32 >> 16) as u16, (kvar as u32 & 0xFFFF) as u16];
    ctx.write_multiple_registers(HOLDING_Q_SETPOINT_ADDR, &words)
        .await
        .unwrap()
        .unwrap();
    write(&mut ctx, HOLDING_Q_MODE_ADDR, 1).await;
    let q = ctx
        .read_holding_registers(HOLDING_Q_SETPOINT_ADDR, 2)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(((u32::from(q[0]) << 16) | u32::from(q[1])) as i32, kvar);
    assert_eq!(holding(&mut ctx, HOLDING_Q_MODE_ADDR).await, 1);
    write(&mut ctx, HOLDING_Q_MODE_ADDR, 2).await;
    assert_eq!(holding(&mut ctx, HOLDING_Q_MODE_ADDR).await, 2);
    assert!(ctx
        .write_single_register(HOLDING_Q_MODE_ADDR, 3)
        .await
        .unwrap()
        .is_err());

    // The command register: a refused move counts on the event counter
    // and the register reads zero; clearing the limit through it works.
    let counter = read(&mut ctx, 43).await;
    write(&mut ctx, HOLDING_COMMAND_ADDR, 7).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_ne!(read(&mut ctx, 43).await, counter, "the refusal was logged");
    assert_eq!(holding(&mut ctx, HOLDING_COMMAND_ADDR).await, 0);
    write(&mut ctx, HOLDING_COMMAND_ADDR, 8).await;
    assert_eq!(holding(&mut ctx, HOLDING_LIMIT_ENABLE_ADDR).await, 0);
    assert!(ctx
        .write_single_register(HOLDING_COMMAND_ADDR, 9)
        .await
        .unwrap()
        .is_err());
}

async fn read(ctx: &mut Context, addr: u16) -> u16 {
    ctx.read_input_registers(addr, 1).await.unwrap().unwrap()[0]
}

/// The phase's acceptance check on Modbus: a poller watching the chain
/// plant sees the hot block's rack fold (base+6) go nonzero and the event
/// counter advance, and the chain's three links appear on their own
/// words in causal order and minutes apart: derate on the racks, the
/// setpoint miss on the block, power limited on the site.
///
/// The poller records when it first *saw* each bit, so it has to start
/// before the first one raises (tick 471). At 600x the 300 ms start-up
/// wait is about 180 ticks, a 5 ms poll about 3, and the links stay
/// seconds apart in wall time even on a loaded machine. At full speed
/// the first poll landed after the derate and a few dozen milliseconds
/// before the miss, and load pushed them into the same poll.
#[tokio::test(flavor = "multi_thread")]
async fn a_poller_sees_the_chain_on_the_alarm_registers() {
    let addr = "127.0.0.1:15503".parse().unwrap();
    let (handle, _sim_task) = sim::start(fixtures::hot_plant(), 600.0, None);
    tokio::spawn(serve(addr, handle));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut ctx = tcp::connect_slave(addr, Slave(1)).await.unwrap();

    let rack_fold = 1000 + 10 * HOT_BLOCK as u16 + 6;
    let block_word = 2000 + 10 * HOT_BLOCK as u16;
    let first_counter = read(&mut ctx, 43).await;
    assert_ne!(read(&mut ctx, rack_fold).await, 0, "hot racks warn at once");
    assert!(
        has_bit(
            u32::from(read(&mut ctx, block_word).await),
            block::HVAC_FAILURE
        ),
        "the failed units show on the block word"
    );

    // Simulated tick at which each link first showed.
    let (mut derate, mut miss, mut limited) = (None, None, None);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    while limited.is_none() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "chain incomplete: derate {derate:?}, miss {miss:?}"
        );
        let t = ctx.read_input_registers(26, 2).await.unwrap().unwrap();
        let tick = (u32::from(t[0]) << 16) | u32::from(t[1]);
        let fold = u32::from(read(&mut ctx, rack_fold).await);
        let word = u32::from(read(&mut ctx, block_word).await);
        let site_word = u32::from(read(&mut ctx, 42).await);
        if derate.is_none() && has_bit(fold, rack::DERATE_ACTIVE) {
            derate = Some(tick);
        }
        if miss.is_none() && has_bit(word, block::SETPOINT_NOT_MET) {
            miss = Some(tick);
        }
        if has_bit(site_word, site::POWER_LIMITED) {
            limited = Some(tick);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (derate, miss, limited) = (derate.unwrap(), miss.unwrap(), limited.unwrap());
    assert!(
        derate + 60 <= miss && miss + 60 <= limited,
        "order and spacing: derate {derate}, miss {miss}, limited {limited}"
    );
    assert_ne!(
        read(&mut ctx, 43).await.wrapping_sub(first_counter),
        0,
        "the event counter advanced"
    );
}

/// Wait until the plant has stepped `tick`.
async fn reach(handle: &sim::SimHandle, tick: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while handle.snapshot.borrow().state.tick < tick {
        assert!(
            tokio::time::Instant::now() < deadline,
            "tick {tick} never came"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A dropout is the wire: the session a poller holds resets on its next
/// request, a new connection is accepted and closed at once, and when the
/// dropout ends the device is back.
#[tokio::test(flavor = "multi_thread")]
async fn a_dropout_resets_the_session_and_refuses_new_ones_until_it_ends() {
    const STORY: &[u8] = br"
schema: 1
name: modbus-dropout
events:
  - at: PT2S
    surface: modbus
    fault: dropout
    duration: PT3S
";
    let addr: SocketAddr = "127.0.0.1:15504".parse().unwrap();
    let params = RunParams {
        speed: 1.0,
        ..test_params()
    };
    let resolved = load(STORY, &PlantConfig::gw01()).unwrap().resolve(params);
    let (handle, _sim_task) = sim::spawn(params, Some(resolved), false);
    tokio::spawn(serve(addr, handle.clone()));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut session = tcp::connect_slave(addr, Slave(1)).await.unwrap();
    assert!(session.read_input_registers(5, 1).await.unwrap().is_ok());

    reach(&handle, 2).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        session.read_input_registers(5, 1).await.is_err(),
        "the session should reset"
    );
    if let Ok(mut fresh) = tcp::connect_slave(addr, Slave(1)).await {
        assert!(
            fresh.read_input_registers(5, 1).await.is_err(),
            "a new session should be closed at once"
        );
    }

    reach(&handle, 5).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut back = tcp::connect_slave(addr, Slave(1)).await.unwrap();
    assert!(back.read_input_registers(5, 1).await.unwrap().is_ok());
}
