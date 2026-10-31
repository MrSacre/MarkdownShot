use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, anyhow};
use ort::environment::Environment;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;

static ENV: OnceLock<Environment> = OnceLock::new();
static WANT_GPU: AtomicBool = AtomicBool::new(false);
static GPU_ON: AtomicBool = AtomicBool::new(false);

pub fn set_gpu(on: bool) {
    let env = std::env::var("MARKDOWNSHOT_GPU").is_ok_and(|v| v == "1");
    WANT_GPU.store(on || env, Ordering::Relaxed);
    GPU_ON.store(false, Ordering::Relaxed);
}

pub fn gpu_active() -> bool {
    GPU_ON.load(Ordering::Relaxed)
}

#[cfg(windows)]
const ORT_LIB: &str = "onnxruntime.dll";
#[cfg(target_os = "macos")]
const ORT_LIB: &str = "libonnxruntime.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
const ORT_LIB: &str = "libonnxruntime.so";

pub fn candidate_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("MARKDOWNSHOT_MODELS") {
        v.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.to_path_buf());
            v.push(dir.join("models"));
            if let Some(root) = dir.parent().and_then(|p| p.parent()) {
                v.push(root.join("models"));
            }
            if let Some(root) = dir.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
                v.push(root.join("models"));
            }
        }
    }
    v.push(PathBuf::from("models"));
    v
}

pub fn find_file(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
}

pub fn models_dir() -> Result<PathBuf> {
    let dirs = candidate_dirs();
    find_file("det.onnx", &dirs)
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .ok_or_else(|| anyhow!("Models not found (det.onnx). Searched in: {dirs:?}"))
}

pub fn environment() -> Result<&'static Environment> {
    if let Some(e) = ENV.get() {
        return Ok(e);
    }
    let dirs = candidate_dirs();
    let lib = std::env::var("ORT_DYLIB_PATH")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| find_file(ORT_LIB, &dirs))
        .ok_or_else(|| anyhow!("{ORT_LIB} not found. Searched in: {dirs:?}"))?;
    log::info!("ONNX Runtime: {}", lib.display());
    let env = ort::init_from(&lib)
        .map_err(|e| anyhow!("Impossible de charger {} : {e}", lib.display()))?
        .with_name("markdownshot")
        .build()
        .map_err(|e| anyhow!("Initialisation ONNX Runtime : {e}"))?;
    let _ = ENV.set(env);
    Ok(ENV.get().expect("environment initialized"))
}

pub fn load_session(path: &Path, threads: usize) -> Result<Session> {
    if WANT_GPU.load(Ordering::Relaxed) {
        match load_gpu(path) {
            Ok(s) => {
                GPU_ON.store(true, Ordering::Relaxed);
                log::info!("{}: GPU (DirectML)", path.file_name().unwrap_or_default().to_string_lossy());
                return Ok(s);
            }
            Err(e) => {
                GPU_ON.store(false, Ordering::Relaxed);
                log::warn!("GPU acceleration unavailable, using the CPU: {e:#}");
            }
        }
    }
    let env = environment()?;
    let mut b = Session::builder(env)
        .map_err(|e| anyhow!("{e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("{e}"))?
        .with_intra_threads(threads)
        .map_err(|e| anyhow!("{e}"))?;
    b.commit_from_file(path)
        .map_err(|e| anyhow!("{e}"))
        .with_context(|| format!("loading model {}", path.display()))
}

pub fn default_threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8)
}

#[cfg(windows)]
fn load_gpu(path: &Path) -> Result<Session> {
    let env = environment()?;
    let mut b = Session::builder(env)
        .map_err(|e| anyhow!("{e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("{e}"))?
        .with_memory_pattern(false)
        .map_err(|e| anyhow!("{e}"))?
        .with_parallel_execution(false)
        .map_err(|e| anyhow!("{e}"))?
        .with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()])
        .map_err(|e| anyhow!("{e}"))?;
    b.commit_from_file(path).map_err(|e| anyhow!("{e}"))
}

#[cfg(not(windows))]
fn load_gpu(_path: &Path) -> Result<Session> {
    Err(anyhow!("DirectML n'existe que sous Windows"))
}
