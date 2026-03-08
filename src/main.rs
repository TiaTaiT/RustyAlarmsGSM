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
mod rtc;
mod sim800;

use crate::alarms_handler::{AlarmStack, AlarmTracker};
use crate::constants::*;
#[cfg(feature = "receiver")]
use crate::hardware::AlarmRelays;
use crate::hardware::{Hardware, PowerState, StatusLeds, SystemSensors};
use crate::rtc::RtcControl;
use crate::sim800::{Command, Sim800Driver, SimEvent};
use static_cell::StaticCell;

// --- Global Signals/Channels ---
static CMD_CHANNEL: Channel<CriticalSectionRawMutex, Command, 4> = Channel::new();
static EVENT_CHANNEL: Channel<CriticalSectionRawMutex, SimEvent, 4> = Channel::new();
static USB_STATE: StaticCell<hardware::UsbResources> = StaticCell::new();

pub static USB_RX_PIPE: Pipe<CriticalSectionRawMutex, 256> = Pipe::new();
pub static USB_TX_PIPE: Pipe<CriticalSectionRawMutex, 1024> = Pipe::new();

struct SystemState {
    alarm_stack: AlarmStack,
    alive_countdown: i32,
    battery_level: u16,
    tamper_detected: bool,
    adc_values: [u16; 4],
    current_alarms: [bool; 4],
    power_connected: bool,
}

static STATE: Mutex<CriticalSectionRawMutex, SystemState> = Mutex::new(SystemState {
    alarm_stack: AlarmStack::new(),
    alive_countdown: 0,
    battery_level: 0,
    tamper_detected: false,
    adc_values: [0; 4],
    current_alarms: [false; 4],
    power_connected: false,
});

static RTC: Mutex<CriticalSectionRawMutex, Option<RtcControl>> = Mutex::new(None);

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

    #[cfg(feature = "receiver")]
    let relays = hw.relays;

    // RTC Init
    {
        let rtc_ctrl = RtcControl::init();
        let mut rtc_lock = RTC.lock().await;
        *rtc_lock = Some(rtc_ctrl);
    }

    info!("Starting Embassy800c on STM32L0...");
    leds.set_system(PowerState::On);
    Timer::after(Duration::from_millis(200)).await;
    leds.set_system(PowerState::Off);

    spawner.spawn(sim800_task(tx, rx, sim_control)).unwrap();
    spawner.spawn(monitor_task(sensors)).unwrap();

    // Pass relays to logic task if receiver
    #[cfg(feature = "receiver")]
    spawner.spawn(logic_task(leds, relays)).unwrap();

    #[cfg(feature = "transmitter")]
    spawner.spawn(logic_task(leds)).unwrap();

    spawner.spawn(system_monitor_task()).unwrap();

    let driver = hw.usb_driver.take().unwrap();
    spawner.spawn(usb_task(driver)).unwrap();
}

#[embassy_executor::task]
async fn usb_task(driver: hardware::BoardUsbDriver) {
    let res = USB_STATE.init(hardware::UsbResources::new());
    let (mut device, serial) = hardware::build_usb(driver, res);
    embassy_futures::join::join(device.run(), async {
        let (mut sender, mut receiver) = serial.split();
        loop {
            sender.wait_connection().await;
            info!("USB connected");

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

            hardware::USB_DISCONNECT_SIGNAL.reset(); // ensure clean state
            embassy_futures::select::select3(rx_fut, tx_fut, hardware::USB_DISCONNECT_SIGNAL.wait()).await;
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
            let b = [
                v[0] > LOW_INTRUSION_THRESHOLD && v[0] < HIGH_INTRUSION_THRESHOLD,
                v[1] > LOW_INTRUSION_THRESHOLD && v[1] < HIGH_INTRUSION_THRESHOLD,
                v[2] > LOW_INTRUSION_THRESHOLD && v[2] < HIGH_INTRUSION_THRESHOLD,
                v[3] > LOW_INTRUSION_THRESHOLD && v[3] < HIGH_INTRUSION_THRESHOLD,
            ];
            (v, b)
        };

        #[cfg(not(feature = "transmitter"))]
        let (values, bools) = ([0u16; 4], [false; 4]);

        // Transmitter: Read Alarms
        #[cfg(feature = "transmitter")]
        {
            let mut state = STATE.lock().await;
            state.alarm_stack.push(&bools);
        }

        // Both: Read System health
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
async fn logic_task(mut leds: StatusLeds) {
    run_logic(leds).await;
}

#[cfg(feature = "receiver")]
#[embassy_executor::task]
async fn logic_task(leds: StatusLeds, mut relays: AlarmRelays) {
    run_logic(leds, &mut relays).await;
}

async fn run_logic(mut leds: StatusLeds, #[cfg(feature = "receiver")] relays: &mut AlarmRelays) {
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
                        leds.set_action(PowerState::On);
                        Timer::after(Duration::from_secs(1)).await;
                        leds.set_action(PowerState::Off);
                    }
                }
                SimEvent::TimeReceived(time) => {
                    let mut rtc = RTC.lock().await;
                    if let Some(ref mut rtc_ctrl) = *rtc {
                        rtc_ctrl.set_time(time);
                    }
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

                        if state.alarm_stack.has_changes() || tick {
                            let bits = state.alarm_stack.export_bits();
                            let str_stack: String<DTMF_PACKET_LENGTH> = bits.iter().collect();
                            state.alive_countdown = ALIVE_PERIOD_MINUTES + 1;

                            if USE_SMS {
                                let time_buf = {
                                    let rtc = RTC.lock().await;
                                    if let Some(ref rtc_ctrl) = *rtc {
                                        let t = rtc_ctrl.get_time();
                                        crate::date_converter::format_gsm_time(&t)
                                    } else {
                                        crate::date_converter::format_gsm_time(
                                            &crate::rtc::GsmTime {
                                                year: 0,
                                                month: 0,
                                                day: 0,
                                                hour: 0,
                                                minute: 0,
                                                second: 0,
                                            },
                                        )
                                    }
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
                leds.set_gsm(PowerState::Off);
                leds.set_error(PowerState::Off);
                leds.set_action(PowerState::Off);

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

// Helper for Transmitter (Just LEDs)
#[cfg(feature = "transmitter")]
async fn visualize_leds(leds: &mut StatusLeds, alarm_str: &str) {
    visualize_common(alarm_str, |idx, state| {
        leds.set_by_index(idx + 1, state); // Map 0..3 to LED 1..4
    })
    .await;
}

// Helper for Receiver (Relays + LEDs)
#[cfg(feature = "receiver")]
async fn visualize_relays(relays: &mut AlarmRelays, leds: &mut StatusLeds, alarm_str: &str) {
    visualize_common(alarm_str, |idx, state| {
        relays.set(idx, state); // Relays 0..3
        leds.set_by_index(idx + 1, state); // LEDs 1..4 (Optional, visual feedback)

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
                "\r\nADC: {}, {}, {}, {}\r\n",
                v[0], v[1], v[2], v[3]
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
