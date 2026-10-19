#![allow(unsafe_code)]
//! Narrow CUDA 13 ABI. Signatures follow NVIDIA's cuda_runtime_api.h,
//! cuda.h, cublas_api.h and cusolverDn.h. No SDK is needed to build.
//! Safety obligations are documented in docs/UNSAFE_BOUNDARIES.md.

use super::{Device, Error, ErrorKind};
use libloading::Library;
use std::{
    ffi::{c_char, c_void},
    fmt,
    marker::PhantomData,
    path::{Path, PathBuf},
    ptr,
    rc::Rc,
    sync::Arc,
};

type GetInt = unsafe extern "system" fn(*mut i32) -> i32;
type Attribute = unsafe extern "system" fn(*mut i32, i32, i32) -> i32;
type SetDevice = unsafe extern "system" fn(i32) -> i32;
type MemInfo = unsafe extern "system" fn(*mut usize, *mut usize) -> i32;
type Allocate = unsafe extern "system" fn(*mut *mut c_void, usize) -> i32;
type CopyMemory = unsafe extern "system" fn(*mut c_void, *const c_void, usize, i32) -> i32;
type HandleCall = unsafe extern "system" fn(*mut c_void) -> i32;
type CreateHandle = unsafe extern "system" fn(*mut *mut c_void) -> i32;
type SetStream = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type EigenWorkspace = unsafe extern "system" fn(
    *mut c_void,
    i32,
    i32,
    i32,
    *const f64,
    i32,
    *const f64,
    *mut i32,
) -> i32;
type Eigen = unsafe extern "system" fn(
    *mut c_void,
    i32,
    i32,
    i32,
    *mut f64,
    i32,
    *mut f64,
    *mut f64,
    i32,
    *mut i32,
) -> i32;
type Gemv = unsafe extern "system" fn(
    *mut c_void,
    i32,
    i32,
    i32,
    *const f64,
    *const f64,
    i32,
    *const f64,
    i32,
    *const f64,
    *mut f64,
    i32,
) -> i32;
type DriverInit = unsafe extern "system" fn(u32) -> i32;
type DriverDevice = unsafe extern "system" fn(*mut i32, i32) -> i32;
type DeviceName = unsafe extern "system" fn(*mut c_char, i32, i32) -> i32;
type DeviceMemory = unsafe extern "system" fn(*mut usize, i32) -> i32;

struct Api {
    _libraries: Vec<Library>,
    runtime_version: GetInt,
    driver_version: GetInt,
    set_device: SetDevice,
    mem_info: MemInfo,
    allocate: Allocate,
    copy: CopyMemory,
    free: HandleCall,
    stream_create: CreateHandle,
    stream_sync: HandleCall,
    stream_destroy: HandleCall,
    solver_create: CreateHandle,
    solver_destroy: HandleCall,
    solver_stream: SetStream,
    eigen_workspace: EigenWorkspace,
    eigen: Eigen,
    blas_create: CreateHandle,
    blas_destroy: HandleCall,
    blas_stream: SetStream,
    gemv: Gemv,
    driver_init: DriverInit,
    device_count: GetInt,
    device_get: DriverDevice,
    attribute: Attribute,
    device_name: DeviceName,
    device_memory: DeviceMemory,
}

fn check(code: i32, operation: &str) -> Result<(), Error> {
    if code == 0 {
        return Ok(());
    }
    let allocation_failed = if operation.starts_with("cublas") {
        code == 3
    } else {
        code == 2
    };
    let kind = match code {
        _ if allocation_failed => ErrorKind::InsufficientMemory,
        35 | 803 | 804 => ErrorKind::RuntimeUnavailable,
        100 => ErrorKind::NoDevice,
        _ => ErrorKind::DeviceFailure,
    };
    Err(Error::new(
        kind,
        format!("{operation} failed (CUDA status {code})"),
    ))
}

fn cancelled(cancel: &dyn Fn() -> bool) -> Result<(), Error> {
    if cancel() {
        Err(Error::new(ErrorKind::Cancelled, "cancelled"))
    } else {
        Ok(())
    }
}

fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, Error> {
    // SAFETY: callers use the exact NVIDIA CUDA 13 ABI typedef for each constant
    // symbol name; Api retains every Library longer than all function pointers.
    unsafe { library.get::<T>(name) }.map(|v| *v).map_err(|e| {
        Error::new(
            ErrorKind::RuntimeUnavailable,
            format!("missing CUDA symbol {}: {e}", String::from_utf8_lossy(name)),
        )
    })
}

fn load_library(path: &Path) -> Result<Library, Error> {
    #[cfg(target_os = "windows")]
    {
        use libloading::os::windows::{
            LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
            Library as WindowsLibrary,
        };
        if !path.is_absolute() {
            return Err(Error::new(
                ErrorKind::RuntimeUnavailable,
                "CUDA library path must be absolute",
            ));
        }
        // SAFETY: only explicitly selected NVIDIA installation/runtime directories
        // and System32 are loaded. The absolute path excludes current-directory
        // search; dependency lookup is restricted to the DLL folder/default dirs.
        unsafe {
            WindowsLibrary::load_with_flags(
                path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        }
        .map(Library::from)
        .map_err(|e| {
            Error::new(
                ErrorKind::RuntimeUnavailable,
                format!("{}: {e}", path.display()),
            )
        })
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: paths are explicit NVIDIA installation roots, except the CUDA
        // driver SONAME resolved by the system loader. Handles remain owned.
        unsafe { Library::new(path) }.map_err(|e| {
            Error::new(
                ErrorKind::RuntimeUnavailable,
                format!("{}: {e}", path.display()),
            )
        })
    }
}

fn roots() -> Result<Vec<PathBuf>, Error> {
    if let Some(value) = std::env::var_os("SECS_CUDA_LIBRARY_PATH") {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(Error::new(
                ErrorKind::RuntimeUnavailable,
                "SECS_CUDA_LIBRARY_PATH must be an absolute directory",
            ));
        }
        return Ok(vec![path]);
    }
    let mut roots = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        roots.push(parent.join("cuda").join("13"));
    }
    for name in ["CUDA_PATH", "CUDA_HOME"] {
        if let Some(path) = std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            #[cfg(target_os = "windows")]
            {
                roots.push(path.join("bin").join(if cfg!(target_arch = "aarch64") {
                    "arm64"
                } else {
                    "x64"
                }));
                roots.push(path.join("bin"));
            }
            #[cfg(target_os = "linux")]
            {
                roots.push(path.join("lib64"));
                roots.push(
                    path.join("targets")
                        .join(if cfg!(target_arch = "aarch64") {
                            "sbsa-linux"
                        } else {
                            "x86_64-linux"
                        })
                        .join("lib"),
                );
            }
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        let toolkit = PathBuf::from(program_files)
            .join("NVIDIA GPU Computing Toolkit")
            .join("CUDA");
        if let Ok(entries) = std::fs::read_dir(toolkit) {
            let mut versions: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|s| s.to_string_lossy().starts_with("v13."))
                })
                .collect();
            versions.sort();
            versions.reverse();
            for version in versions {
                roots.push(version.join("bin").join(if cfg!(target_arch = "aarch64") {
                    "arm64"
                } else {
                    "x64"
                }));
                roots.push(version.join("bin"));
            }
        }
    }
    #[cfg(target_os = "linux")]
    for root in [
        "/usr/local/cuda/lib64",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/usr/lib64",
    ] {
        roots.push(PathBuf::from(root));
    }
    roots.dedup();
    Ok(roots)
}

fn driver_library() -> Result<Library, Error> {
    #[cfg(target_os = "windows")]
    let path = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::NoDevice,
                "Windows system directory is unavailable",
            )
        })?
        .join("System32")
        .join("nvcuda.dll");
    #[cfg(target_os = "linux")]
    let path = PathBuf::from("libcuda.so.1");
    load_library(&path).map_err(|e| Error::new(ErrorKind::NoDevice, e.detail))
}

impl Api {
    fn load() -> Result<Self, Error> {
        let roots = roots()?;
        // An override is strict, making diagnostics and CPU-only tests independent
        // of any globally installed toolkit.
        let override_set = std::env::var_os("SECS_CUDA_LIBRARY_PATH").is_some();
        let driver = if override_set {
            None
        } else {
            Some(driver_library()?)
        };
        let mut last = None;
        for root in roots {
            match Self::from_root(&root, driver.as_ref()) {
                Ok(api) => return Ok(api),
                Err(error) => last = Some(error),
            }
        }
        Err(last.unwrap_or_else(|| {
            Error::new(
                ErrorKind::RuntimeUnavailable,
                "CUDA 13 runtime libraries were not found",
            )
        }))
    }

    fn from_root(root: &Path, _probed_driver: Option<&Library>) -> Result<Self, Error> {
        #[cfg(target_os = "windows")]
        let names = [
            "cudart64_13.dll",
            "cublasLt64_13.dll",
            "cublas64_13.dll",
            "cusparse64_12.dll",
            "cusolver64_12.dll",
        ];
        #[cfg(target_os = "linux")]
        let names = [
            "libcudart.so.13",
            "libcublasLt.so.13",
            "libcublas.so.13",
            "libcusparse.so.12",
            "libcusolver.so.12",
        ];
        let mut libraries: Vec<Library> = names
            .iter()
            .map(|name| load_library(&root.join(name)))
            .collect::<Result<_, _>>()?;
        libraries.push(driver_library()?);
        let runtime = &libraries[0];
        let blas = &libraries[2];
        let solver = &libraries[4];
        let driver = &libraries[5];
        macro_rules! get {
            ($lib:expr, $name:literal) => {
                symbol($lib, concat!($name, "\0").as_bytes())?
            };
        }
        Ok(Self {
            runtime_version: get!(runtime, "cudaRuntimeGetVersion"),
            driver_version: get!(runtime, "cudaDriverGetVersion"),
            set_device: get!(runtime, "cudaSetDevice"),
            mem_info: get!(runtime, "cudaMemGetInfo"),
            allocate: get!(runtime, "cudaMalloc"),
            copy: get!(runtime, "cudaMemcpy"),
            free: get!(runtime, "cudaFree"),
            stream_create: get!(runtime, "cudaStreamCreate"),
            stream_sync: get!(runtime, "cudaStreamSynchronize"),
            stream_destroy: get!(runtime, "cudaStreamDestroy"),
            solver_create: get!(solver, "cusolverDnCreate"),
            solver_destroy: get!(solver, "cusolverDnDestroy"),
            solver_stream: get!(solver, "cusolverDnSetStream"),
            eigen_workspace: get!(solver, "cusolverDnDsyevd_bufferSize"),
            eigen: get!(solver, "cusolverDnDsyevd"),
            blas_create: get!(blas, "cublasCreate_v2"),
            blas_destroy: get!(blas, "cublasDestroy_v2"),
            blas_stream: get!(blas, "cublasSetStream_v2"),
            gemv: get!(blas, "cublasDgemv_v2"),
            driver_init: get!(driver, "cuInit"),
            device_count: get!(driver, "cuDeviceGetCount"),
            device_get: get!(driver, "cuDeviceGet"),
            attribute: get!(driver, "cuDeviceGetAttribute"),
            device_name: get!(driver, "cuDeviceGetName"),
            device_memory: get!(driver, "cuDeviceTotalMem_v2"),
            _libraries: libraries,
        })
    }

    fn bind(&self, device: i32) -> Result<(), Error> {
        // SAFETY: the ordinal is a checked i32. CUDA validates its range and
        // returns an error for nonexistent or removed devices.
        check(unsafe { (self.set_device)(device) }, "cudaSetDevice")
    }
}

#[derive(Clone)]
pub struct Runtime {
    api: Arc<Api>,
}
impl fmt::Debug for Runtime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Runtime")
            .field("cuda_major", &13)
            .finish_non_exhaustive()
    }
}

impl Runtime {
    pub fn load() -> Result<Self, Error> {
        if !cfg!(target_pointer_width = "64") {
            return Err(Error::new(
                ErrorKind::UnsupportedPlatform,
                "CUDA requires a 64-bit host",
            ));
        }
        let api = Api::load()?;
        let mut runtime = 0;
        let mut driver = 0;
        check(
            // SAFETY: both output pointers refer to live initialized i32 values.
            unsafe { (api.runtime_version)(&mut runtime) },
            "cudaRuntimeGetVersion",
        )?;
        check(
            // SAFETY: the loaded ABI expects an i32 output and the pointer is writable.
            unsafe { (api.driver_version)(&mut driver) },
            "cudaDriverGetVersion",
        )?;
        if runtime / 1000 != 13 || driver < 13000 {
            return Err(Error::new(
                ErrorKind::RuntimeUnavailable,
                format!("CUDA 13 runtime/driver required (runtime {runtime}, driver {driver})"),
            ));
        }
        Ok(Self { api: Arc::new(api) })
    }

    pub fn devices(&self) -> Result<Vec<Device>, Error> {
        // SAFETY: flags=0 is the documented driver initialization mode.
        check(unsafe { (self.api.driver_init)(0) }, "cuInit")?;
        let mut count = 0;
        check(
            // SAFETY: count points to a live writable i32.
            unsafe { (self.api.device_count)(&mut count) },
            "cuDeviceGetCount",
        )?;
        if !(0..=128).contains(&count) {
            return Err(Error::new(
                ErrorKind::DeviceFailure,
                "invalid CUDA device count",
            ));
        }
        let mut devices = Vec::new();
        for ordinal in 0..count {
            let mut device = 0;
            check(
                // SAFETY: ordinal is in the enumerated range and output is writable.
                unsafe { (self.api.device_get)(&mut device, ordinal) },
                "cuDeviceGet",
            )?;
            let attribute = |kind| -> Result<u32, Error> {
                let mut value = 0;
                check(
                    // SAFETY: these constant CUDA device attributes use an i32 output;
                    // device is the valid handle returned by cuDeviceGet.
                    unsafe { (self.api.attribute)(&mut value, kind, device) },
                    "cuDeviceGetAttribute",
                )?;
                u32::try_from(value).map_err(|_| {
                    Error::new(ErrorKind::DeviceFailure, "negative CUDA device attribute")
                })
            };
            let mut name = [0u8; 256];
            let mut memory = 0usize;
            check(
                // SAFETY: the 256-byte buffer is writable and its exact capacity is
                // supplied. cuDeviceGetName writes at most that many bytes.
                unsafe { (self.api.device_name)(name.as_mut_ptr().cast(), 256, device) },
                "cuDeviceGetName",
            )?;
            check(
                // SAFETY: CUDA's size_t matches usize on the supported 64-bit targets.
                unsafe { (self.api.device_memory)(&mut memory, device) },
                "cuDeviceTotalMem_v2",
            )?;
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            devices.push(Device {
                id: ordinal as u32,
                name: String::from_utf8_lossy(&name[..end]).into_owned(),
                compute_major: attribute(75)?,
                compute_minor: attribute(76)?,
                multiprocessors: attribute(16)?,
                memory_bytes: memory as u64,
                integrated: attribute(18)? != 0,
            });
        }
        Ok(devices)
    }

    pub fn factor(
        &self,
        device: u32,
        ata: &[f64],
        atb: &[f64],
        cancel: &dyn Fn() -> bool,
    ) -> Result<SpectralSystem, Error> {
        cancelled(cancel)?;
        let n = validate_system(ata, atb)?;
        let device = i32::try_from(device)
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "invalid device ordinal"))?;
        let session = Rc::new(Session::new(self.api.clone(), device)?);
        let matrix_bytes = ata
            .len()
            .checked_mul(8)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "matrix size overflow"))?;
        let fixed_bytes = matrix_bytes
            .checked_add(n * 8 * 4 + 4)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "workspace size overflow"))?;
        session.admit(fixed_bytes)?;
        let matrix = Buffer::new(&session, matrix_bytes)?;
        let eigenvalues = Buffer::new(&session, n * 8)?;
        let vector = Buffer::new(&session, n * 8)?;
        let output = Buffer::new(&session, n * 8)?;
        let info = Buffer::new(&session, 4)?;
        // A is symmetric and both triangles are checked, so its row-major bytes
        // also represent the same matrix in column-major cuSOLVER storage.
        matrix.upload(ata)?;
        let mut workspace_len = 0i32;
        check(
            // SAFETY: n/lda and all device buffers have been checked. jobz=1 requests
            // vectors, uplo=0 selects the lower triangle. The output is a writable i32.
            unsafe {
                (session.api.eigen_workspace)(
                    session.solver,
                    1,
                    0,
                    n as i32,
                    matrix.f64_ptr(),
                    n as i32,
                    eigenvalues.f64_ptr(),
                    &mut workspace_len,
                )
            },
            "cusolverDnDsyevd_bufferSize",
        )?;
        let workspace_bytes = usize::try_from(workspace_len)
            .ok()
            .and_then(|n| n.checked_mul(8))
            .filter(|&n| n > 0)
            .ok_or_else(|| {
                Error::new(ErrorKind::DeviceFailure, "invalid cuSOLVER workspace size")
            })?;
        session.admit(fixed_bytes.checked_add(workspace_bytes).ok_or_else(|| {
            Error::new(ErrorKind::InsufficientMemory, "CUDA workspace overflow")
        })?)?;
        let workspace = Buffer::new(&session, workspace_bytes)?;
        cancelled(cancel)?;
        session.bind()?;
        check(
            // SAFETY: matrix, eigenvalue, workspace and info allocations are owned,
            // correctly sized and stay alive until the stream has completed.
            unsafe {
                (session.api.eigen)(
                    session.solver,
                    1,
                    0,
                    n as i32,
                    matrix.f64_ptr(),
                    n as i32,
                    eigenvalues.f64_ptr(),
                    workspace.f64_ptr(),
                    workspace_len,
                    info.ptr.cast(),
                )
            },
            "cusolverDnDsyevd",
        )?;
        session.synchronize()?;
        cancelled(cancel)?;
        let info_value = info.download_i32()?;
        if info_value != 0 {
            return Err(Error::new(
                ErrorKind::DeviceFailure,
                format!("cuSOLVER eigendecomposition did not converge (info {info_value})"),
            ));
        }
        let eigenvalues_host = eigenvalues.download(n)?;
        if eigenvalues_host.iter().any(|v| !v.is_finite()) {
            return Err(Error::new(
                ErrorKind::DeviceFailure,
                "nonfinite CUDA eigenvalues",
            ));
        }
        vector.upload(atb)?;
        session.gemv(true, n, &matrix, &vector, &output)?;
        session.synchronize()?;
        let projected = output.download(n)?;
        if projected.iter().any(|v| !v.is_finite()) {
            return Err(Error::new(
                ErrorKind::DeviceFailure,
                "nonfinite CUDA projection",
            ));
        }
        cancelled(cancel)?;
        // Scratch/eigenvalue/info buffers can be released now; V and vectors
        // remain on the GPU throughout all adaptive shifts.
        drop(workspace);
        drop(eigenvalues);
        drop(info);
        Ok(SpectralSystem {
            session,
            matrix,
            vector,
            output,
            eigenvalues: eigenvalues_host,
            projected,
            _not_send: PhantomData,
        })
    }
}

fn validate_system(ata: &[f64], atb: &[f64]) -> Result<usize, Error> {
    let n = atb.len();
    if n == 0
        || n > 4096
        || n.checked_mul(n) != Some(ata.len())
        || ata.iter().chain(atb).any(|v| !v.is_finite())
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ridge matrix must be finite, square, and have dimension 1..4096",
        ));
    }
    for i in 0..n {
        for j in 0..i {
            let a = ata[i * n + j];
            let b = ata[j * n + i];
            if a != b && (a - b).abs() > 1e-12 * a.abs().max(b.abs()).max(1.0) {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    "ridge matrix must be symmetric",
                ));
            }
        }
    }
    Ok(n)
}

struct Session {
    api: Arc<Api>,
    device: i32,
    stream: *mut c_void,
    solver: *mut c_void,
    blas: *mut c_void,
    _not_send: PhantomData<Rc<()>>,
}
impl Session {
    fn new(api: Arc<Api>, device: i32) -> Result<Self, Error> {
        api.bind(device)?;
        let mut session = Self {
            api,
            device,
            stream: ptr::null_mut(),
            solver: ptr::null_mut(),
            blas: ptr::null_mut(),
            _not_send: PhantomData,
        };
        let mut stream = ptr::null_mut();
        check(
            // SAFETY: output points to a writable, initially-null local slot.
            // Session takes ownership only after successful creation.
            unsafe { (session.api.stream_create)(&mut stream) },
            "cudaStreamCreate",
        )?;
        session.stream = stream;
        let mut solver = ptr::null_mut();
        check(
            // SAFETY: output points to a writable, initially-null local slot.
            // A failed call's output never becomes an owned Session handle.
            unsafe { (session.api.solver_create)(&mut solver) },
            "cusolverDnCreate",
        )?;
        session.solver = solver;
        check(
            // SAFETY: both handles were just successfully created on this device.
            unsafe { (session.api.solver_stream)(session.solver, session.stream) },
            "cusolverDnSetStream",
        )?;
        let mut blas = ptr::null_mut();
        check(
            // SAFETY: output points to a writable, initially-null local slot.
            // Ownership transfers only after the API reports success.
            unsafe { (session.api.blas_create)(&mut blas) },
            "cublasCreate_v2",
        )?;
        session.blas = blas;
        check(
            // SAFETY: both handles are live, owned, and on this device.
            unsafe { (session.api.blas_stream)(session.blas, session.stream) },
            "cublasSetStream_v2",
        )?;
        Ok(session)
    }
    fn bind(&self) -> Result<(), Error> {
        self.api.bind(self.device)
    }
    fn synchronize(&self) -> Result<(), Error> {
        self.bind()?;
        check(
            // SAFETY: the owned stream is live and bound to the selected device.
            unsafe { (self.api.stream_sync)(self.stream) },
            "cudaStreamSynchronize",
        )
    }
    fn memory_info(&self) -> Result<(usize, usize), Error> {
        self.bind()?;
        let mut free = 0;
        let mut total = 0;
        check(
            // SAFETY: both size_t output pointers refer to live usize values.
            unsafe { (self.api.mem_info)(&mut free, &mut total) },
            "cudaMemGetInfo",
        )?;
        Ok((free, total))
    }
    fn admit(&self, needed: usize) -> Result<(), Error> {
        let (free, _) = self.memory_info()?;
        if needed > free / 2 || needed > 1024 * 1024 * 1024 {
            return Err(Error::new(
                ErrorKind::InsufficientMemory,
                format!(
                    "CUDA workspace requires {needed} bytes; free {free}, reserve limit {}",
                    free / 2
                ),
            ));
        }
        Ok(())
    }
    fn gemv(
        &self,
        transpose: bool,
        n: usize,
        matrix: &Buffer,
        x: &Buffer,
        y: &Buffer,
    ) -> Result<(), Error> {
        self.bind()?;
        let alpha = 1.0;
        let beta = 0.0;
        check(
            // SAFETY: all three allocations have checked f64 dimensions; n fits i32,
            // lda=n, unit vector strides, host alpha/beta, no overlapping buffers.
            unsafe {
                (self.api.gemv)(
                    self.blas,
                    i32::from(transpose),
                    n as i32,
                    n as i32,
                    &alpha,
                    matrix.f64_ptr(),
                    n as i32,
                    x.f64_ptr(),
                    1,
                    &beta,
                    y.f64_ptr(),
                    1,
                )
            },
            "cublasDgemv_v2",
        )
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if self.bind().is_err() {
            return;
        }
        if !self.stream.is_null() {
            // SAFETY: the stream is owned and completion precedes handle teardown.
            let _ = unsafe { (self.api.stream_sync)(self.stream) };
        }
        if !self.blas.is_null() {
            // SAFETY: this BLAS handle is owned and destroyed at most once.
            let _ = unsafe { (self.api.blas_destroy)(self.blas) };
        }
        if !self.solver.is_null() {
            // SAFETY: this solver handle is owned and destroyed at most once.
            let _ = unsafe { (self.api.solver_destroy)(self.solver) };
        }
        if !self.stream.is_null() {
            // SAFETY: the stream is owned; no library operation is submitted later.
            let _ = unsafe { (self.api.stream_destroy)(self.stream) };
        }
    }
}

struct Buffer {
    session: Rc<Session>,
    ptr: *mut c_void,
    bytes: usize,
    _not_send: PhantomData<Rc<()>>,
}
impl Buffer {
    fn new(session: &Rc<Session>, bytes: usize) -> Result<Self, Error> {
        if bytes == 0 {
            return Err(Error::new(ErrorKind::InvalidInput, "empty CUDA allocation"));
        }
        session.bind()?;
        let mut ptr = ptr::null_mut();
        check(
            // SAFETY: the output slot is writable and the checked byte count is
            // nonzero. CUDA owns the returned device allocation, freed by Buffer.
            unsafe { (session.api.allocate)(&mut ptr, bytes) },
            "cudaMalloc",
        )?;
        if ptr.is_null() {
            return Err(Error::new(
                ErrorKind::DeviceFailure,
                "CUDA returned a null allocation",
            ));
        }
        Ok(Self {
            session: session.clone(),
            ptr,
            bytes,
            _not_send: PhantomData,
        })
    }
    fn f64_ptr(&self) -> *mut f64 {
        self.ptr.cast()
    }
    fn upload(&self, values: &[f64]) -> Result<(), Error> {
        let bytes = values
            .len()
            .checked_mul(8)
            .filter(|&n| n <= self.bytes)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "CUDA upload exceeds allocation"))?;
        self.session.bind()?;
        check(
            // SAFETY: initialized host values cover bytes; destination owns at least
            // bytes. Synchronous HtoD keeps the borrowed host slice alive until copied.
            unsafe { (self.session.api.copy)(self.ptr, values.as_ptr().cast(), bytes, 1) },
            "cudaMemcpy HtoD",
        )
    }
    fn download(&self, n: usize) -> Result<Vec<f64>, Error> {
        let bytes = n
            .checked_mul(8)
            .filter(|&n| n <= self.bytes)
            .ok_or_else(|| {
                Error::new(ErrorKind::InvalidInput, "CUDA readback exceeds allocation")
            })?;
        self.session.bind()?;
        let mut values = vec![0.0; n];
        check(
            // SAFETY: source owns bytes and destination contains n initialized f64s.
            // The producer stream is synchronized before every readback call.
            unsafe { (self.session.api.copy)(values.as_mut_ptr().cast(), self.ptr, bytes, 2) },
            "cudaMemcpy DtoH",
        )?;
        Ok(values)
    }
    fn download_i32(&self) -> Result<i32, Error> {
        if self.bytes < 4 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "CUDA info buffer is too short",
            ));
        }
        self.session.bind()?;
        let mut value = 0i32;
        check(
            // SAFETY: source and destination both cover one i32; stream is complete.
            unsafe { (self.session.api.copy)((&mut value as *mut i32).cast(), self.ptr, 4, 2) },
            "cudaMemcpy info",
        )?;
        Ok(value)
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        if self.session.synchronize().is_ok() {
            // SAFETY: the pointer came from cudaMalloc and is freed exactly once.
            // The owning stream has completed; on device loss resources may leak
            // but are never reused or made accessible through the safe API.
            let _ = unsafe { (self.session.api.free)(self.ptr) };
        }
    }
}

/// Owned device factorization. It cannot leave the creating worker thread.
pub struct SpectralSystem {
    session: Rc<Session>,
    matrix: Buffer,
    vector: Buffer,
    output: Buffer,
    eigenvalues: Vec<f64>,
    projected: Vec<f64>,
    _not_send: PhantomData<Rc<()>>,
}
impl fmt::Debug for SpectralSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpectralSystem")
            .field("dimension", &self.eigenvalues.len())
            .field("device", &self.session.device)
            .finish_non_exhaustive()
    }
}
impl SpectralSystem {
    pub fn solve(&mut self, lambda: f64, cancel: &dyn Fn() -> bool) -> Result<Vec<f64>, Error> {
        self.solve_projected(lambda, &self.projected, cancel)
    }

    /// Solve a correction right-hand side using the same owned factorization.
    pub fn solve_rhs(
        &mut self,
        lambda: f64,
        rhs: &[f64],
        cancel: &dyn Fn() -> bool,
    ) -> Result<Vec<f64>, Error> {
        cancelled(cancel)?;
        if !lambda.is_finite()
            || lambda <= 0.0
            || rhs.len() != self.eigenvalues.len()
            || rhs.iter().any(|v| !v.is_finite())
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "invalid ridge correction shift/right-hand side",
            ));
        }
        self.vector.upload(rhs)?;
        self.session
            .gemv(true, rhs.len(), &self.matrix, &self.vector, &self.output)?;
        self.session.synchronize()?;
        cancelled(cancel)?;
        let projected = self.output.download(rhs.len())?;
        self.solve_projected(lambda, &projected, cancel)
    }

    fn solve_projected(
        &self,
        lambda: f64,
        projected: &[f64],
        cancel: &dyn Fn() -> bool,
    ) -> Result<Vec<f64>, Error> {
        cancelled(cancel)?;
        if !lambda.is_finite() || lambda <= 0.0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "ridge shift must be positive and finite",
            ));
        }
        let shifted: Vec<_> = self
            .eigenvalues
            .iter()
            .zip(projected)
            .map(|(&d, &c)| {
                let denominator = d + lambda;
                if denominator > 0.0 && denominator.is_finite() {
                    c / denominator
                } else {
                    f64::NAN
                }
            })
            .collect();
        if shifted.iter().any(|v| !v.is_finite()) {
            return Err(Error::new(
                ErrorKind::DeviceFailure,
                "invalid shifted CUDA eigenvalues",
            ));
        }
        self.vector.upload(&shifted)?;
        self.session.gemv(
            false,
            shifted.len(),
            &self.matrix,
            &self.vector,
            &self.output,
        )?;
        self.session.synchronize()?;
        cancelled(cancel)?;
        let result = self.output.download(shifted.len())?;
        if result.iter().any(|v| !v.is_finite()) {
            return Err(Error::new(
                ErrorKind::DeviceFailure,
                "nonfinite CUDA solution",
            ));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matrix_validation_rejects_invalid_shapes_and_numbers() {
        assert!(validate_system(&[], &[]).is_err());
        assert!(validate_system(&[1.0, 2.0], &[1.0]).is_err());
        assert!(validate_system(&[f64::NAN], &[1.0]).is_err());
        assert!(validate_system(&[1.0], &[f64::INFINITY]).is_err());
        assert!(validate_system(&[1.0, 2.0, 3.0, 1.0], &[1.0, 1.0]).is_err());
        assert_eq!(
            validate_system(&[2.0, 1.0, 1.0, 2.0], &[1.0, 1.0]).unwrap(),
            2
        );
    }
    #[test]
    fn vendor_allocation_failures_have_the_correct_reason() {
        assert_eq!(
            check(3, "cublasCreate_v2").unwrap_err().kind,
            ErrorKind::InsufficientMemory
        );
        assert_eq!(
            check(2, "cusolverDnCreate").unwrap_err().kind,
            ErrorKind::InsufficientMemory
        );
        assert_eq!(
            check(2, "cudaMalloc").unwrap_err().kind,
            ErrorKind::InsufficientMemory
        );
        assert_eq!(
            check(3, "cudaSetDevice").unwrap_err().kind,
            ErrorKind::DeviceFailure
        );
    }
    #[test]
    fn cancellation_is_not_a_device_failure() {
        assert_eq!(cancelled(&|| true).unwrap_err().kind, ErrorKind::Cancelled);
        assert!(cancelled(&|| false).is_ok());
    }
}

#[cfg(test)]
mod hardware_tests {
    use super::*;
    #[test]
    #[ignore = "requires an idle eligible CUDA 13 GPU and runtime"]
    fn native_cuda_repeated_factorization_releases_device_buffers() {
        let runtime = Runtime::load().unwrap();
        let device = runtime
            .devices()
            .unwrap()
            .into_iter()
            .find(|d| !d.integrated && (d.compute_major, d.compute_minor) >= (7, 5))
            .unwrap();
        let n = 512;
        let mut matrix = vec![0.0; n * n];
        for i in 0..n {
            matrix[i * n + i] = 1.0 + i as f64 / n as f64;
        }
        let rhs = vec![1.0; n];
        let run = || {
            let mut factor = runtime.factor(device.id, &matrix, &rhs, &|| false).unwrap();
            let solution = factor.solve(0.5, &|| false).unwrap();
            assert!(solution.iter().all(|x| x.is_finite()));
        };
        run();
        let monitor = Session::new(runtime.api.clone(), device.id as i32).unwrap();
        let before = monitor.memory_info().unwrap().0;
        for _ in 0..16 {
            run();
        }
        let after = monitor.memory_info().unwrap().0;
        eprintln!("CUDA memory free before={before} after={after}");
        assert!(
            after.saturating_add(16 * 1024 * 1024) >= before,
            "device buffers leaked across repeated factorizations"
        );
    }
    #[test]
    #[ignore = "requires an eligible CUDA 13 GPU and runtime"]
    fn native_cuda_spectral_solution_matches_an_analytic_system() {
        let runtime = Runtime::load().unwrap();
        let device = runtime
            .devices()
            .unwrap()
            .into_iter()
            .find(|d| !d.integrated && (d.compute_major, d.compute_minor) >= (7, 5))
            .unwrap();
        eprintln!("CUDA hardware test: {}", device.name);
        for _ in 0..16 {
            let mut system = runtime
                .factor(device.id, &[4.0, 1.0, 1.0, 3.0], &[1.0, 2.0], &|| false)
                .unwrap();
            let answer = system.solve(0.5, &|| false).unwrap();
            for (actual, expected) in answer.iter().zip([1.5 / 14.75, 8.0 / 14.75]) {
                assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
            }
            let rhs = [2.0, 4.0];
            let doubled = system.solve_rhs(0.5, &rhs, &|| false).unwrap();
            for (a, b) in doubled.iter().zip(&answer) {
                assert!((a - 2.0 * b).abs() < 1e-12);
            }
            assert_eq!(
                system.solve_rhs(0.5, &[1.0], &|| false).unwrap_err().kind,
                ErrorKind::InvalidInput
            );
            assert_eq!(
                system
                    .solve_rhs(f64::NAN, &rhs, &|| false)
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidInput
            );
            assert_eq!(
                system
                    .solve_rhs(0.5, &[f64::NAN, 1.0], &|| false)
                    .unwrap_err()
                    .kind,
                ErrorKind::InvalidInput
            );
            assert_eq!(
                system.solve(0.0, &|| false).unwrap_err().kind,
                ErrorKind::InvalidInput
            );
            assert_eq!(
                system.solve(0.5, &|| true).unwrap_err().kind,
                ErrorKind::Cancelled
            );
        }
    }
}
