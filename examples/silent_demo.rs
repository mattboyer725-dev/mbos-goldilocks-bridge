use mbos_goldilocks_bridge::*;

struct LocalSource { i: u64 }
impl SilentConnection for LocalSource {
    fn next_field_element(&mut self) -> Option<u64> {
        if self.i < 10 { let v = self.i; self.i += 1; Some(v * 17) } else { None }
    }
}

fn main() {
    let mut loop_ = AdaptiveLoop::start();
    let mut src = LocalSource { i: 0 };
    loop_.run_silent(&mut src, 100);
    println!("generation: {}", loop_.generation());
    println!("bias:       {:#x}", loop_.learned_bias());
    println!("agents:     {}", loop_.agents_created());
    println!("state:      {:#x}", loop_.current_state());
}
