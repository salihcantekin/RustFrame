@echo off
REM Force DX12 backend for transparency support
set WGPU_BACKEND=dx12
set RUST_LOG=info

echo Starting RustFrame with DX12 backend...
target\release\rustframe_iced.exe
pause
