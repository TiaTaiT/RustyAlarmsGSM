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
mod constants;
mod custom_strings;
mod date_converter;
mod gsm_time_converter;
mod hardware;
mod phone_book;
mod sim800;

#[cfg(test)]
mod tests;

use crate::alarms_handler::{AlarmStack, AlarmTracker};
use crate::constants::*;
#[cfg(feature = "receiver")]
use crate::hardware::AlarmRelays;
use crate::hardware::{Hardware, LedInterface, PowerState, RtcControl, StatusLeds, SystemSensors};
#[cfg(feature = "receiver")]
use crate::hardware::RelayInterface;
use crate::sim800::{Command, Sim800Driver, SimEvent};
use crate::hardware::Rtc;
use static_cell::StaticCell;

// --- Global Signals/Channels ---
static CMD_CHANNEL: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
static EVENT_CHANNEL: Channel<CriticalSectionRawMutex, SimEvent, 4> = Channel::new();
static USB_STATE: StaticCell<hardware::UsbResources> = StaticCell::new();
static RTC_STATE: StaticCell<Mutex<CriticalSectionRawMutex, RtcControl>> = StaticCell::new();
pub static USB_RX_PIPE: Pipe<CriticalSectionRawMutex, 256> = Pipe::new();
pub static USB_TX_PIPE: Pipe<CriticalSectionRawMutex, 1024> = Pipe::new();

struct SystemState {
    alarm_stack: AlarmStack,
    alive_countdown: i32,
    pending_dtmf: Option<String<DTMF_PACKET_LENGTH>>,
    retry_countdown: Option<u32>,
    pending_alive_message: bool,
    battery_level: u16,
    tamper_detected: bool,
    adc_values: [u16; 3],
    current_alarms: [bool; 4],
    power_connected: bool,
}

static STATE: Mutex<CriticalSectionRawMutex, SystemState> = Mutex::new(SystemState {
    alarm_stack: AlarmStack::new(),
    alive_countdown: 0,
    pending_dtmf: None,
    retry_countdown: None,
    pending_alive_message: false,
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
            state.alarm_stack.push(&bools);
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
            // --- CASE 1: EVENTS (Both) ---
            Either3::First(event) => match event {
                SimEvent::SmsReceived { message, .. } => {
                    if let Some(alarm_str) = custom_strings::extract_before_delimiter(&message, ";")
                    {
                        if alarm_str.len() == ALARMS_MESSAGE_STRING_LENGTH {
                            #[cfg(feature = "transmitter")]
                            visualize_leds(&mut leds, alarm_str).await;

                            #[cfg(feature = "receiver")]
                            visualize_relays(relays, &mut leds, alarm_str).await;

                            watchdog_deadline =
                                Some(Instant::now() + Duration::from_secs(255 * 60));
                        }
                    }
                }
                SimEvent::DtmfReceived(c) => {
                    if dtmf_buffer.push(c).is_ok() {
                        if dtmf_buffer.len() == DTMF_PACKET_LENGTH {
                            #[cfg(feature = "transmitter")]
                            visualize_leds(&mut leds, &dtmf_buffer).await;

                            #[cfg(feature = "receiver")]
                            visualize_relays(relays, &mut leds, &dtmf_buffer).await;

                            watchdog_deadline =
                                Some(Instant::now() + Duration::from_secs(255 * 60));
                            dtmf_buffer.clear();
                        }
                    }
                }
                SimEvent::CallEnded => {
                    dtmf_buffer.clear();
                }
                SimEvent::CallReceived { number } => {
                    CMD_CHANNEL
                        .send(Command::HandleIncomingCall {
                            phone_number: number,
                        })
                        .await;
                }
                SimEvent::CallExecuted(success) => {
                    if success {
                        let mut state = STATE.lock().await;
                        state.alarm_stack.acknowledge_export();
                        state.pending_dtmf = None;
                        state.retry_countdown = None;
                        state.pending_alive_message = false;

                        leds.set_alarm3(PowerState::On);
                        Timer::after(Duration::from_secs(1)).await;
                        leds.set_alarm3(PowerState::Off);
                    } else {
                        let mut state = STATE.lock().await;
                        if state.pending_dtmf.is_some() {
                            state.retry_countdown = Some(CALLBACK_PERIOD_MINUTES);
                        }
                    }
                }
                SimEvent::TimeReceived(time) => {
                    let mut rtc = rtc.lock().await;
                    rtc.set_time(time);
                }
            },

            // --- CASE 2: SENDER LOGIC TICK ---
            Either3::Second(_) => {
                next_sender_tick += Duration::from_secs(60);

                #[cfg(feature = "transmitter")]
                {
                    let mut pending_dtmf: Option<String<DTMF_PACKET_LENGTH>> = None;
                    let mut pending_sms: Option<String<SIM800_LINE_BUFFER_SIZE>> = None;
                    let mut is_sms = false;

                    {
                        let mut state = STATE.lock().await;
                        let tick = state.alive_countdown <= 0;

                        if let Some(countdown) = state.retry_countdown.as_mut() {
                            if *countdown > 0 {
                                *countdown -= 1;
                            }
                        }

                        let should_retry = matches!(state.retry_countdown, Some(0));
                        let should_send_new = state.pending_dtmf.is_none()
                            && !state.pending_alive_message
                            && (state.alarm_stack.has_changes() || tick);

                        if should_retry {
                            pending_dtmf = state.pending_dtmf.clone();
                            if pending_dtmf.is_some() {
                                state.retry_countdown = Some(CALLBACK_PERIOD_MINUTES);
                            } else {
                                state.retry_countdown = None;
                            }
                        } else if should_send_new {
                            let bits = state.alarm_stack.export_bits();
                            let str_stack: String<DTMF_PACKET_LENGTH> = bits.iter().collect();
                            state.alive_countdown = ALIVE_PERIOD_MINUTES + 1;

                            if use_sms {
                                let time_buf = {
                                    let rtc = rtc.lock().await;
                                    let t = rtc.get_time();
                                    crate::date_converter::format_gsm_time(&t)
                                };
                                let mut msg = String::<SIM800_LINE_BUFFER_SIZE>::new();
                                use core::fmt::Write;
                                let _ = write!(
                                    msg,
                                    "{}{}{}{}{}",
                                    SMS_PREFIX,
                                    SMS_DIVIDER,
                                    str_stack,
                                    SMS_DIVIDER,
                                    time_buf.as_str()
                                );
                                pending_sms = Some(msg);
                                is_sms = true;
                            } else {
                                state.pending_dtmf = Some(str_stack.clone());
                                state.retry_countdown = None;
                                state.pending_alive_message = tick;
                                pending_dtmf = Some(str_stack);
                            }
                        }
                        if !tick {
                            state.alive_countdown -= 1;
                        }
                    }

                    if is_sms {
                        if let Some(msg) = pending_sms {
                            CMD_CHANNEL
                                .send(Command::SendAlarmSms { message: msg })
                                .await;
                        }
                    } else if let Some(dtmf) = pending_dtmf {
                        CMD_CHANNEL.send(Command::CallAlarmWithDtmf { dtmf }).await;
                    }
                }
            }

            // --- CASE 3: WATCHDOG EXPIRED ---
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

    let mut alarm_chars = ['\0'; ALARMS_MESSAGE_STRING_LENGTH];
    for (i, c) in alarm_str
        .chars()
        .take(ALARMS_MESSAGE_STRING_LENGTH)
        .enumerate()
    {
        alarm_chars[i] = c;
    }

    let mut temp_stack = AlarmStack::new();
    temp_stack.import_bits(alarm_chars);
    let matrix = temp_stack.get_stack_view();

    for row in matrix.iter() {
        for (idx, &active) in row.iter().enumerate() {
            set_output(
                idx,
                if active {
                    PowerState::On
                } else {
                    PowerState::Off
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
    let mut reply = String::<128>::new();
    use core::fmt::Write;

    let state = STATE.lock().await;

    match cmd.trim_end() {
#[cfg(feature = "transmitter")]
        "_alarms" => {
            let a = state.current_alarms;
            let _ = write!(
                reply,
                "\r\nAlarms: {}{}{}{}\r\n",
                a[0] as u8, a[1] as u8, a[2] as u8, a[3] as u8
            );
        }
#[cfg(feature = "transmitter")]
        "_adc" => {
            let v = state.adc_values;
            let _ = write!(
                reply,
                "\r\nADC: {}, {}, {}\r\n",
                v[0], v[1], v[2]
            );
        }
#[cfg(feature = "receiver")]
        "_relays" => {
            let bits = RELAY_STATES.load(core::sync::atomic::Ordering::Relaxed);
            let _ = write!(
                reply,
                "\r\nRelays: {}{}{}{}\r\n",
                bits & 1,
                (bits >> 1) & 1,
                (bits >> 2) & 1,
                (bits >> 3) & 1
            );
        }
        "_battery" => {
            let _ = write!(reply, "\r\nBattery: {} mV\r\n", state.battery_level);
        }
        "_power" => {
            let p = if state.power_connected { "Connected" } else { "Disconnected" };
            let _ = write!(reply, "\r\nPower: {}\r\n", p);
        }
        "_tamper" => {
            let t = if state.tamper_detected { "Open" } else { "Closed" };
            let _ = write!(reply, "\r\nTamper: {}\r\n", t);
        }
        _ => {
            let _ = write!(reply, "\r\nUnknown MCU command: {}\r\n", cmd);
        }
    }

    let bytes = reply.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let space = core::cmp::min(bytes.len() - offset, 64);
        let _ = USB_TX_PIPE.write(&bytes[offset..offset + space]).await;
        offset += space;
    }
}
