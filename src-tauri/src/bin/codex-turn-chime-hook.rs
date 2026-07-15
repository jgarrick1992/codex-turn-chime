fn main() {
    // A monitoring failure must never block or alter the Codex turn.
    let _ = codex_turn_chime_lib::hook_helper::run_from_reader(std::io::stdin().lock());
}
