use mbos_goldilocks_bridge::*;

struct LocalSource { i: u64 }
impl LocalConnection for LocalSource {
    fn next_field_element(&mut self) -> Option<u64> {
        if self.i < 12 { let v = self.i * 137; self.i += 1; Some(v) } else { None }
    }
}

fn main() {
    let mut loop_ = AdaptiveLoop::with_target(500);
    let mut src = LocalSource { i: 0 };
    loop_.run_local(&mut src, 30);
    println!("generation:      {}", loop_.generation());
    println!("bias:            {:#x}", loop_.learned_bias());
    println!("adaptation rate: {}", loop_.adaptation_rate());
    println!("state:           {:#x}", loop_.current_state());
    println!("local only:      {}", loop_.is_local_only());
}
