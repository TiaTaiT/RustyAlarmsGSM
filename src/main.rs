#![no_std]
#![no_main]

use defmt::{info, warn};
use defmt_rtt as _;
use panic_probe as _;

use embassy_executor::Spawner;
use embassy_futures::select::{Either3, select3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::pipe::Pipe;
use embassy_time::{Duration, Instant, Timer};
use heapless::String;

mod alarms_handler;
mod app_logic;
mod constants;
mod custom_strings;
mod date_converter;
mod gsm_time_converter;
mod hardware;
mod mcu_commands;
mod phone_book;
mod sim800;
mod sim800_logic;
mod sim800_parser;
mod visualization;

#[cfg(test)]
mod tests;

use crate::alarms_handler::{AlarmStack, AlarmTracker};
use crate::app_logic::{LogicAction, LogicCommand, LogicEvent, LogicState, handle_event, handle_sender_tick};
use crate::constants::*;
#[cfg(feature = "receiver")]
use crate::hardware::AlarmRelays;
use crate::hardware::{Hardware, LedInterface, PowerState, RtcControl, StatusLeds, SystemSensors};
#[cfg(feature = "receiver")]
use crate::hardware::RelayInterface;
use crate::mcu_commands::{SystemSnapshot, format_mcu_reply};
use crate::sim800::{Command, Sim800Driver, SimEvent};
use crate::hardware::Rtc;
use static_cell::StaticCell;
use crate::visualization::{VisualizationState, build_visualization_frames};

// --- Global Signals/Channels ---
static CMD_CHANNEL: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
static EVENT_CHANNEL: Channel<CriticalSectionRawMutex, SimEvent, 4> = Channel::new();
static USB_STATE: StaticCell<hardware::UsbResources> = StaticCell::new();
static RTC_STATE: StaticCell<Mutex<CriticalSectionRawMutex, RtcControl>> = StaticCell::new();
pub static USB_RX_PIPE: Pipe<CriticalSectionRawMutex, 256> = Pipe::new();
pub static USB_TX_PIPE: Pipe<CriticalSectionRawMutex, 1024> = Pipe::new();

struct SystemState {
    logic: LogicState,
    battery_level: u16,
    tamper_detected: bool,
    adc_values: [u16; 3],
    current_alarms: [bool; 4],
    power_connected: bool,
}

static STATE: Mutex<CriticalSectionRawMutex, SystemState> = Mutex::new(SystemState {
    logic: LogicState::new(),
    battery_level: 0,
    tamper_detected: false,
    adc_values: [0; 3],
    current_alarms: [false; 4],
    power_connected: false,
});

#[cfg(feature = "receiver")]
static RELAY_STATES: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // 1. Initialize new Hardware struct
    let mut hw: Hardware = hardware::init();

    let mut leds = hw.leds;
    let sim_control = hw.modem_ctrl;
    let sensors = hw.sensors;
    let tx = hw.modem_tx;
    let rx = hw.modem_rx;

    #[cfg(feature = "transmitter")]
    let alarms_ctrl = hw.alarms_ctrl;
    
    #[cfg(feature = "receiver")]
    let relays = hw.relays;

    let rtc = RTC_STATE.init(Mutex::new(RtcControl::init()));

    info!("Starting Embassy800c on STM32L0...");
    leds.set_system(PowerState::On);
    Timer::after(Duration::from_millis(200)).await;
    leds.set_system(PowerState::Off);

    spawner.spawn(sim800_task(tx, rx, sim_control).unwrap());
    spawner.spawn(monitor_task(sensors).unwrap());

    #[cfg(feature = "transmitter")]
    spawner.spawn(logic_task(leds, alarms_ctrl, rtc).unwrap());

    #[cfg(feature = "receiver")]
    spawner.spawn(logic_task(leds, relays, rtc).unwrap());

    spawner.spawn(system_monitor_task().unwrap());

    let driver = hw.usb_driver.take().unwrap();
    spawner.spawn(usb_task(driver).unwrap());
}

#[embassy_executor::task]
async fn usb_task(driver: hardware::BoardUsbDriver) {
    let res = USB_STATE.init(hardware::UsbResources::new());
    let (mut device, serial) = hardware::build_usb(driver, res);
    embassy_futures::join::join(device.run(), async {
        let (mut sender, mut receiver) = serial.split();
        loop {
            // Wait for physical USB connection
            while !hardware::USB_CONNECTED.load(core::sync::atomic::Ordering::Relaxed) {
                hardware::USB_STATE_SIGNAL.wait().await;
            }

            // Wait for terminal connection (DTR), but abort if physically disconnected
            let wait_conn = sender.wait_connection();
            let wait_disconn = async {
                loop {
                    hardware::USB_STATE_SIGNAL.wait().await;
                    if !hardware::USB_CONNECTED.load(core::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                }
            };

            match embassy_futures::select::select(wait_conn, wait_disconn).await {
                embassy_futures::select::Either::Second(_) => continue,
                _ => {}
            }

            // Double check to be absolutely sure
            if !hardware::USB_CONNECTED.load(core::sync::atomic::Ordering::Relaxed) {
                continue;
            }

            info!("USB connected");
            CMD_CHANNEL.send(Command::UsbConnected).await;

            let rx_fut = async {
                let mut buf = [0u8; 64];
                loop {
                    match receiver.read_packet(&mut buf).await {
                        Ok(n) => {
                            for &b in &buf[..n] {
                                let _ = USB_RX_PIPE.try_write(&[b]);
                            }
                        }
                        Err(_) => break,
                    }
                }
            };

            let tx_fut = async {
                let mut buf = [0u8; 64];
                loop {
                    let n = USB_TX_PIPE.read(&mut buf).await;
                    if n > 0 {
                        if sender.write_packet(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                }
            };

            let disconnect_fut = async {
                if !hardware::USB_CONNECTED.load(core::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                loop {
                    hardware::USB_STATE_SIGNAL.wait().await;
                    if !hardware::USB_CONNECTED.load(core::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                }
            };

            embassy_futures::select::select3(rx_fut, tx_fut, disconnect_fut).await;
            CMD_CHANNEL.send(Command::UsbDisconnected).await;
            info!("USB disconnected");
        }
    })
    .await;
}

#[embassy_executor::task]
async fn sim800_task(
    tx: hardware::ModemTx,
    rx: hardware::ModemRx,
    control: hardware::ModemControl,
) {
    let mut driver = Sim800Driver::new(tx, rx, control);

    CMD_CHANNEL.send(Command::UpdateTime).await;

    driver
        .run(CMD_CHANNEL.receiver(), EVENT_CHANNEL.sender())
        .await;
}

#[embassy_executor::task]
async fn monitor_task(mut sensors: SystemSensors) {
    loop {
        #[cfg(feature = "transmitter")]
        let (values, bools) = {
            let v = sensors.read_alarms().await;
            let tamper_state = !sensors.is_housing_open();
            let b = [
                v[0] > LOW_INTRUSION_THRESHOLD && v[0] < HIGH_INTRUSION_THRESHOLD,
                v[1] > LOW_INTRUSION_THRESHOLD && v[1] < HIGH_INTRUSION_THRESHOLD,
                v[2] > LOW_INTRUSION_THRESHOLD && v[2] < HIGH_INTRUSION_THRESHOLD,
                tamper_state,
            ];
            (v, b)
        };

        #[cfg(not(feature = "transmitter"))]
        let (values, bools) = ([0u16; 3], [false; 4]);

        #[cfg(feature = "transmitter")]
        {
            let mut state = STATE.lock().await;
            state.logic.alarm_stack.push(&bools);
        }

        let battery = sensors.read_battery_voltage().await;
        let tamper = sensors.is_housing_open();
        let power = sensors.is_power_connected();

        {
            let mut state = STATE.lock().await;
            state.adc_values = values;
            state.current_alarms = bools;
            state.battery_level = battery;
            if tamper && !state.tamper_detected {
                warn!("TAMPER DETECTED!");
            }
            state.tamper_detected = tamper;
            state.power_connected = power;
        }

        Timer::after(Duration::from_millis(500)).await;
    }
}

#[cfg(feature = "transmitter")]
#[embassy_executor::task]
async fn logic_task(
    leds: StatusLeds,
    mut alarms_ctrl: hardware::AlarmsControl,
    rtc: &'static Mutex<CriticalSectionRawMutex, RtcControl>,
) {
    alarms_ctrl.set_pullup(PowerState::On);
    let use_sms = alarms_ctrl.is_sms_enabled();
    run_logic(leds, rtc, use_sms).await;
}

#[cfg(feature = "receiver")]
#[embassy_executor::task]
async fn logic_task(
    leds: StatusLeds,
    mut relays: AlarmRelays,
    rtc: &'static Mutex<CriticalSectionRawMutex, RtcControl>,
) {
    run_logic(leds, &mut relays, rtc).await;
}

#[cfg(feature = "transmitter")]
async fn visualize_leds<L: LedInterface>(leds: &mut L, alarm_str: &str) {
    visualize_common(alarm_str, |idx, state| {
        leds.set_by_index(idx + 1, state);
    })
    .await;
}

#[cfg(feature = "receiver")]
async fn visualize_relays<L: LedInterface, R: RelayInterface>(
    relays: &mut R,
    leds: &mut L,
    alarm_str: &str,
) {
    visualize_common(alarm_str, |idx, state| {
        relays.set(idx, state);
        leds.set_by_index(idx + 1, state);

        let mut bits = RELAY_STATES.load(core::sync::atomic::Ordering::Relaxed);
        if state == PowerState::On {
            bits |= 1 << idx;
        } else {
            bits &= !(1 << idx);
        }
        RELAY_STATES.store(bits, core::sync::atomic::Ordering::Relaxed);
    })
    .await;
}

async fn run_logic(
    mut leds: impl LedInterface,
    #[cfg(feature = "receiver")]
    relays: &mut impl RelayInterface,
    rtc: &'static Mutex<CriticalSectionRawMutex, RtcControl>,
    #[cfg(feature = "transmitter")]
    use_sms: bool
) {
    let mut watchdog_deadline: Option<Instant> = None;
    let mut dtmf_buffer = String::<DTMF_PACKET_LENGTH>::new();
    let mut next_sender_tick = Instant::now() + Duration::from_secs(60);

    loop {
        let watchdog_fut = async {
            if let Some(deadline) = watchdog_deadline {
                Timer::at(deadline).await;
                true
            } else {
                core::future::pending::<bool>().await
            }
        };
        let sender_fut = Timer::at(next_sender_tick);
        let event_fut = EVENT_CHANNEL.receive();

        match select3(event_fut, sender_fut, watchdog_fut).await {
            Either3::First(event) => {
                let actions = {
                    let mut state = STATE.lock().await;
                    handle_event(&mut state.logic, &mut dtmf_buffer, map_sim_event(event))
                };
                apply_logic_actions(
                    &actions,
                    &mut leds,
                    #[cfg(feature = "receiver")]
                    relays,
                    rtc,
                    &mut watchdog_deadline,
                )
                .await;
            }

            Either3::Second(_) => {
                next_sender_tick += Duration::from_secs(60);

                #[cfg(feature = "transmitter")]
                {
                    let current_time = {
                        let rtc = rtc.lock().await;
                        rtc.get_time()
                    };
                    let actions = {
                        let mut state = STATE.lock().await;
                        handle_sender_tick(&mut state.logic, use_sms, Some(&current_time))
                    };
                    apply_logic_actions(
                        &actions,
                        &mut leds,
                        #[cfg(feature = "receiver")]
                        relays,
                        rtc,
                        &mut watchdog_deadline,
                    )
                    .await;
                }
            }

            Either3::Third(_) => {
                info!("Watchdog expired. Resetting outputs.");
                leds.set_system(PowerState::Off);
                leds.set_alarm1(PowerState::Off);
                leds.set_alarm2(PowerState::Off);
                leds.set_alarm3(PowerState::Off);

                #[cfg(feature = "receiver")]
                {
                    relays.set_all(PowerState::Off);
                    RELAY_STATES.store(0, core::sync::atomic::Ordering::Relaxed);
                }

                watchdog_deadline = None;
            }
        }
    }
}

async fn visualize_common<F>(alarm_str: &str, mut set_output: F)
where
    F: FnMut(usize, PowerState),
{
    info!("Visualizing: {}", alarm_str);
    let Some(frames) = build_visualization_frames(alarm_str) else {
        return;
    };

    for row in frames.iter() {
        for (idx, &active) in row.iter().enumerate() {
            set_output(
                idx,
                match active {
                    VisualizationState::On => PowerState::On,
                    VisualizationState::Off => PowerState::Off,
                },
            );
        }
        Timer::after(Duration::from_secs(3)).await;
    }
}

#[embassy_executor::task]
async fn system_monitor_task() {
    loop {
        Timer::after(Duration::from_secs(
            SYSTEM_MONITOR_PERIOD_HOURS as u64 * 3600,
        ))
        .await;
        CMD_CHANNEL.send(Command::UpdateTime).await;
    }
}

pub async fn execute_mcu_command(cmd: &str) {
    let snapshot = {
        let state = STATE.lock().await;
        SystemSnapshot {
            battery_level: state.battery_level,
            tamper_detected: state.tamper_detected,
            power_connected: state.power_connected,
            #[cfg(feature = "transmitter")]
            adc_values: state.adc_values,
            #[cfg(feature = "transmitter")]
            current_alarms: state.current_alarms,
            #[cfg(feature = "receiver")]
            relay_bits: RELAY_STATES.load(core::sync::atomic::Ordering::Relaxed),
        }
    };

    let reply = format_mcu_reply(&snapshot, cmd);

    let bytes = reply.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let space = core::cmp::min(bytes.len() - offset, 64);
        let _ = USB_TX_PIPE.write(&bytes[offset..offset + space]).await;
        offset += space;
    }
}

async fn apply_logic_actions(
    actions: &[LogicAction],
    leds: &mut impl LedInterface,
    #[cfg(feature = "receiver")]
    relays: &mut impl RelayInterface,
    rtc: &'static Mutex<CriticalSectionRawMutex, RtcControl>,
    watchdog_deadline: &mut Option<Instant>,
) {
    for action in actions {
        match action {
            LogicAction::Visualize(alarm_str) => {
                #[cfg(feature = "transmitter")]
                visualize_leds(leds, alarm_str).await;

                #[cfg(feature = "receiver")]
                visualize_relays(relays, leds, alarm_str).await;
            }
            LogicAction::SendCommand(cmd) => {
                CMD_CHANNEL.send(map_logic_command(cmd.clone())).await;
            }
            LogicAction::BlinkAlarm3 => {
                leds.set_alarm3(PowerState::On);
                Timer::after(Duration::from_secs(1)).await;
                leds.set_alarm3(PowerState::Off);
            }
            LogicAction::UpdateRtc(time) => {
                let mut rtc = rtc.lock().await;
                rtc.set_time(*time);
            }
            LogicAction::SetWatchdog(timeout) => {
                *watchdog_deadline = timeout
                    .map(|secs| Instant::now() + Duration::from_secs(secs));
            }
        }
    }
}

fn map_sim_event(event: SimEvent) -> LogicEvent {
    match event {
        SimEvent::SmsReceived { number, message } => LogicEvent::SmsReceived { number, message },
        SimEvent::CallReceived { number } => LogicEvent::CallReceived { number },
        SimEvent::DtmfReceived(c) => LogicEvent::DtmfReceived(c),
        SimEvent::CallEnded => LogicEvent::CallEnded,
        SimEvent::CallExecuted(success) => LogicEvent::CallExecuted(success),
        SimEvent::TimeReceived(time) => LogicEvent::TimeReceived(time),
    }
}

fn map_logic_command(command: LogicCommand) -> Command {
    match command {
        LogicCommand::HandleIncomingCall { phone_number } => Command::HandleIncomingCall { phone_number },
        LogicCommand::SendAlarmSms { message } => Command::SendAlarmSms { message },
        LogicCommand::CallAlarmWithDtmf { dtmf } => Command::CallAlarmWithDtmf { dtmf },
    }
}
