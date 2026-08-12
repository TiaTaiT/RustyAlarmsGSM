use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PowerState {
    On,
    Off,
}

pub trait ModemControlInterface {
    fn set_power_key(&mut self, state: PowerState);
    fn set_dc_power(&mut self, state: PowerState);
}

pub trait ModemTxInterface {
    async fn write(&mut self, buf: &[u8]) -> Result<(), ()>;
}

pub trait ModemRxInterface {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()>;
    async fn read_until_idle(&mut self, buf: &mut [u8]) -> Result<usize, ()>;
}

#[derive(Clone, Default)]
pub struct ModemTx {
    writes: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl ModemTx {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn writes(&self) -> Vec<Vec<u8>> {
        self.writes.lock().unwrap().clone()
    }
}

impl ModemTxInterface for ModemTx {
    async fn write(&mut self, buf: &[u8]) -> Result<(), ()> {
        self.writes.lock().unwrap().push(buf.to_vec());
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct ModemRx {
    state: Arc<Mutex<MockRxState>>,
}

#[derive(Default)]
struct MockRxState {
    buffer: VecDeque<u8>,
}

impl ModemRx {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_idle_read(&self, data: &[u8]) {
        self.state.lock().unwrap().buffer.extend(data);
    }

    pub fn push_read(&self, data: &[u8]) {
        self.state.lock().unwrap().buffer.extend(data);
    }
}

impl ModemRxInterface for ModemRx {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        let mut state = self.state.lock().unwrap();
        if state.buffer.is_empty() {
            return Err(());
        }
        let count = buf.len().min(state.buffer.len());
        for i in 0..count {
            buf[i] = state.buffer.pop_front().unwrap();
        }
        Ok(count)
    }

    async fn read_until_idle(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        self.read(buf).await
    }
}

#[derive(Clone, Default)]
pub struct ModemControl {
    pub power_key_states: Vec<PowerState>,
    pub dc_power_states: Vec<PowerState>,
}

impl ModemControlInterface for ModemControl {
    fn set_power_key(&mut self, state: PowerState) {
        self.power_key_states.push(state);
    }

    fn set_dc_power(&mut self, state: PowerState) {
        self.dc_power_states.push(state);
    }
}

pub struct Eeprom;

impl Eeprom {
    pub fn read_alive_period() -> u32 {
        90
    }

    pub fn read_alive_period_delay() -> u32 {
        20
    }

    pub fn write_alive_period(_value: u32) {}

    pub fn write_alive_period_delay(_value: u32) {}
}