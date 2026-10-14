//! Capability views corresponding to upstream `rewapi/interfaces.py`.
use crate::*;

pub trait RewStatus {
    fn is_running(&mut self) -> bool;
    fn version(&mut self) -> Option<String>;
    fn is_pro(&self) -> bool;
}
pub trait DeviceLister {
    fn output_devices(&mut self) -> Vec<DeviceInfo>;
    fn input_devices(&mut self) -> Vec<DeviceInfo>;
}
pub trait LevelMonitor {
    fn start(&mut self, on_level: LevelCallback) -> Result<(), RewError>;
    fn stop(&mut self);
    fn play_pink(&mut self, role: &str, output: u32, level_db: f64) -> Result<(), RewError>;
    fn stop_generator(&mut self);
}
pub trait Measurer {
    /// Cancel returns completed channels; the terminal progress phase is cancelled.
    fn run_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: u32,
        on_progress: Option<ProgressCallback>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, RewError>;
}
pub trait IRProvider {
    fn get_ir(&mut self, uuid: &str) -> Result<(Vec<f32>, u32), RewError>;
}

impl<T: Transport> RewStatus for RewClient<T> {
    fn is_running(&mut self) -> bool {
        RewClient::is_running(self)
    }
    fn version(&mut self) -> Option<String> {
        RewClient::version(self)
    }
    fn is_pro(&self) -> bool {
        RewClient::is_pro(self)
    }
}
impl<T: Transport> DeviceLister for RewClient<T> {
    fn output_devices(&mut self) -> Vec<DeviceInfo> {
        RewClient::output_devices(self)
    }
    fn input_devices(&mut self) -> Vec<DeviceInfo> {
        RewClient::input_devices(self)
    }
}
impl<T: Transport> LevelMonitor for RewClient<T> {
    fn start(&mut self, on_level: LevelCallback) -> Result<(), RewError> {
        RewClient::start(self, on_level)
    }
    fn stop(&mut self) {
        RewClient::stop(self);
    }
    fn play_pink(&mut self, role: &str, output: u32, level_db: f64) -> Result<(), RewError> {
        RewClient::play_pink(self, role, output, level_db)
    }
    fn stop_generator(&mut self) {
        RewClient::stop_generator(self);
    }
}
impl<T: Transport> Measurer for RewClient<T> {
    fn run_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: u32,
        on_progress: Option<ProgressCallback>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, RewError> {
        RewClient::run_position(self, plan, pos, on_progress, cancel)
    }
}
impl<T: Transport> IRProvider for RewClient<T> {
    fn get_ir(&mut self, uuid: &str) -> Result<(Vec<f32>, u32), RewError> {
        RewClient::get_ir(self, uuid)
    }
}
impl<S: IrSource> RewStatus for FakeRew<S> {
    fn is_running(&mut self) -> bool {
        FakeRew::is_running(self)
    }
    fn version(&mut self) -> Option<String> {
        FakeRew::version(self)
    }
    fn is_pro(&self) -> bool {
        FakeRew::is_pro(self)
    }
}
impl<S: IrSource> DeviceLister for FakeRew<S> {
    fn output_devices(&mut self) -> Vec<DeviceInfo> {
        FakeRew::output_devices(self)
    }
    fn input_devices(&mut self) -> Vec<DeviceInfo> {
        FakeRew::input_devices(self)
    }
}
impl<S: IrSource> LevelMonitor for FakeRew<S> {
    fn start(&mut self, on_level: LevelCallback) -> Result<(), RewError> {
        FakeRew::start(self, on_level);
        Ok(())
    }
    fn stop(&mut self) {
        FakeRew::stop(self);
    }
    fn play_pink(&mut self, role: &str, output: u32, level_db: f64) -> Result<(), RewError> {
        FakeRew::play_pink(self, role, output, level_db);
        Ok(())
    }
    fn stop_generator(&mut self) {
        FakeRew::stop_generator(self);
    }
}
impl<S: IrSource> Measurer for FakeRew<S> {
    fn run_position(
        &mut self,
        plan: &MeasurementPlan,
        pos: u32,
        on_progress: Option<ProgressCallback>,
        cancel: &CancelToken,
    ) -> Result<Vec<MeasurementResult>, RewError> {
        FakeRew::run_position(self, plan, pos, on_progress, cancel)
    }
}
impl<S: IrSource> IRProvider for FakeRew<S> {
    fn get_ir(&mut self, uuid: &str) -> Result<(Vec<f32>, u32), RewError> {
        FakeRew::get_ir(self, uuid)
    }
}
