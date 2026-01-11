use windows::Graphics::Capture::GraphicsCaptureSession;

fn main() {
    // Test if SetIncludeSecondaryWindows method exists
    println!("Testing GraphicsCaptureSession API...");
    
    // This will compile if the method exists
    // let session: GraphicsCaptureSession = todo!();
    // session.SetIncludeSecondaryWindows(true).unwrap();
}
