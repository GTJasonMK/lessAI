pub(crate) trait BoundaryGuard: Default {
    fn observe_char(&mut self, chars: &[char], index: usize);
    fn should_cut(&mut self, hit_boundary: bool) -> bool;
    fn reset_after_cut(&mut self);
}

#[derive(Default)]
pub(crate) struct NoopBoundaryGuard;

impl BoundaryGuard for NoopBoundaryGuard {
    fn observe_char(&mut self, _chars: &[char], _index: usize) {}

    fn should_cut(&mut self, hit_boundary: bool) -> bool {
        hit_boundary
    }

    fn reset_after_cut(&mut self) {}
}

#[derive(Default)]
pub(crate) struct TexBraceBoundaryGuard {
    brace_depth: usize,
    pending_boundary: bool,
}

impl BoundaryGuard for TexBraceBoundaryGuard {
    fn observe_char(&mut self, chars: &[char], index: usize) {
        let ch = chars[index];
        if ch == '{' && !is_tex_char_escaped(chars, index) {
            self.brace_depth = self.brace_depth.saturating_add(1);
            return;
        }
        if ch == '}' && !is_tex_char_escaped(chars, index) {
            self.brace_depth = self.brace_depth.saturating_sub(1);
        }
    }

    fn should_cut(&mut self, hit_boundary: bool) -> bool {
        if hit_boundary && self.brace_depth != 0 {
            self.pending_boundary = true;
            return false;
        }
        if self.brace_depth == 0 && (hit_boundary || self.pending_boundary) {
            self.pending_boundary = false;
            return true;
        }
        false
    }

    fn reset_after_cut(&mut self) {
        self.brace_depth = 0;
        self.pending_boundary = false;
    }
}

fn is_tex_char_escaped(chars: &[char], index: usize) -> bool {
    if index == 0 {
        return false;
    }

    let mut backslashes = 0usize;
    let mut pos = index;
    while pos > 0 {
        pos -= 1;
        if chars[pos] != '\\' {
            break;
        }
        backslashes = backslashes.saturating_add(1);
    }
    backslashes % 2 == 1
}
