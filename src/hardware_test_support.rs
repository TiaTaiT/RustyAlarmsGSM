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
    idle_reads: VecDeque<Result<Vec<u8>, ()>>,
    reads: VecDeque<Result<Vec<u8>, ()>>,
}

impl ModemRx {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_idle_read(&self, data: &[u8]) {
        self.state.lock().unwrap().idle_reads.push_back(Ok(data.to_vec()));
    }

    pub fn push_read(&self, data: &[u8]) {
        self.state.lock().unwrap().reads.push_back(Ok(data.to_vec()));
    }
}

impl ModemRxInterface for ModemRx {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        let next = self.state.lock().unwrap().reads.pop_front().ok_or(())??;
        let len = next.len().min(buf.len());
        buf[..len].copy_from_slice(&next[..len]);
        Ok(len)
    }

    async fn read_until_idle(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        let next = self.state.lock().unwrap().idle_reads.pop_front().ok_or(())??;
        let len = next.len().min(buf.len());
        buf[..len].copy_from_slice(&next[..len]);
        Ok(len)
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
