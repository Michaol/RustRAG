use ort::execution_providers::CUDAExecutionProvider; fn main() { let _ = CUDAExecutionProvider::default().build(); }
