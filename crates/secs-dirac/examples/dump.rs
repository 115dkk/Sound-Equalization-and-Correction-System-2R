#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
//! Inspect measured IRs without interpreting private or opaque project fields.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: dump <file.liveproject>")?;
    let bytes = std::fs::read(path)?;
    let project = secs_dirac::parse(&bytes, &secs_dirac::Limits::default())?;
    println!("name: {}", project.name);
    println!("application: {}", project.application);
    println!("microphone: {}", project.microphone);
    println!("rate: {}", project.sample_rate);
    println!("positions: {}", project.positions);
    for (index, channel) in project.channels.iter().enumerate() {
        println!("channel {index}: {} ({:?})", channel.name, channel.kind);
    }
    for ir in project.measured() {
        let (peak_index, peak) = ir
            .samples
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .ok_or("empty measured IR")?;
        let energy: f64 = ir.samples.iter().map(|x| x * x).sum();
        println!(
            "IR position={} channel={} length={} peak_index={} peak={:.17e} energy={:.17e}",
            ir.position,
            ir.channel,
            ir.samples.len(),
            peak_index,
            peak,
            energy
        );
    }
    Ok(())
}
