use crate::hardware::{ModemControl, ModemRx, ModemTx};
use crate::sim800::{Sim800Driver, SimError, SimEvent, TestEventSink};
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

struct NoopWaker;

impl Wake for NoopWaker {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(NoopWaker));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);

    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn make_driver() -> (Sim800Driver<ModemTx, ModemRx, ModemControl>, ModemTx, ModemRx) {
    let tx = ModemTx::new();
    let rx = ModemRx::new();
    let driver = Sim800Driver::new(tx.clone(), rx.clone(), ModemControl::default());
    (driver, tx, rx)
}

#[test]
fn send_cmd_wait_ok_returns_ok_on_ok() {
    let (mut driver, tx, rx) = make_driver();
    rx.push_idle_read(b"OK\r\n");

    let result = block_on(driver.test_send_cmd_wait_ok("AT", 50));

    assert_eq!(result, Ok(()));
    assert_eq!(tx.writes(), vec![b"AT".to_vec(), b"\r\n".to_vec()]);
}

#[test]
fn send_cmd_wait_ok_returns_at_error_on_error() {
    let (mut driver, _tx, rx) = make_driver();
    rx.push_idle_read(b"ERROR\r\n");

    let result = block_on(driver.test_send_cmd_wait_ok("AT", 50));

    assert_eq!(result, Err(SimError::AtError));
}

#[test]
fn send_cmd_wait_ok_collects_phonebook_entries() {
    let (mut driver, _tx, rx) = make_driver();
    rx.push_idle_read(b"+CPBR: 1,\"+123456\",129,\"A\"\r\nOK\r\n");

    let result = block_on(driver.test_send_cmd_wait_ok("AT+CPBR=1", 50));

    assert_eq!(result, Ok(()));
    assert_eq!(driver.test_phonebook_first().as_deref(), Some("+123456"));
}

#[test]
fn read_line_handles_chunked_crlf_data() {
    let (mut driver, _tx, rx) = make_driver();
    rx.push_idle_read(b"HEL");
    rx.push_idle_read(b"LO\r\n");

    let line = block_on(driver.test_read_line()).unwrap();

    assert_eq!(line.as_str(), "HELLO");
}

#[test]
fn read_line_processes_sms_urc_and_emits_event() {
    let (mut driver, _tx, rx) = make_driver();
    let events = TestEventSink::new();
    driver.test_set_event_channel(events.clone());
    rx.push_idle_read(b"+CMT: \"+998\",\"\",\"\"\r\nhello\r\n");

    let line = block_on(driver.test_read_line_and_process_urcs()).unwrap();
    let event = events.try_receive().unwrap();

    assert_eq!(line.as_str(), "+CMT: \"+998\",\"\",\"\"");
    match event {
        SimEvent::SmsReceived { number, message } => {
            assert_eq!(number.as_str(), "+998");
            assert_eq!(message.as_str(), "hello");
        }
        _ => panic!("unexpected event"),
    }
}

#[test]
fn read_line_processes_dtmf_urc_and_emits_event() {
    let (mut driver, _tx, rx) = make_driver();
    let events = TestEventSink::new();
    driver.test_set_event_channel(events.clone());
    rx.push_idle_read(b"+DTMF: #\r\n");

    let line = block_on(driver.test_read_line_and_process_urcs()).unwrap();
    let event = events.try_receive().unwrap();

    assert_eq!(line.as_str(), "+DTMF: #");
    match event {
        SimEvent::DtmfReceived('#') => {}
        _ => panic!("unexpected event"),
    }
}
