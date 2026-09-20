use crate::text_input::TextInput;

/// Which of the page's four authentication methods is on screen. `Enter` and the form below
/// the tab strip are read off this, and the four `:command`s in `input::ex` are nothing but a
/// way to land on the page with one of these selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginMethod {
    #[default]
    Qr,
    Password,
    Sms,
    Sign,
}

impl LoginMethod {
    /// The tab strip, left to right.
    pub const ALL: [Self; 4] = [Self::Qr, Self::Password, Self::Sms, Self::Sign];

    /// The tab's own label. Short on purpose: the box is half the terminal wide, so four full
    /// names would wrap next to each other; [`Self::heading`] is the long one, drawn above the
    /// method's own content.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Qr => "二维码",
            Self::Password => "密码",
            Self::Sms => "短信",
            Self::Sign => "签到",
        }
    }

    /// The full name of the method, for the line above its form.
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Qr => "二维码登录",
            Self::Password => "账号密码登录",
            Self::Sms => "短信验证码登录",
            Self::Sign => "每日签到",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Qr => 0,
            Self::Password => 1,
            Self::Sms => 2,
            Self::Sign => 3,
        }
    }

    /// The inputs `Tab` walks through. The QR and check-in methods have no form at all, which
    /// is why this is a slice rather than a fixed pair: `Tab` has nothing to move there.
    pub const fn fields(self) -> &'static [LoginField] {
        match self {
            Self::Qr | Self::Sign => &[],
            Self::Password => &[LoginField::Account, LoginField::Password],
            Self::Sms => &[LoginField::Phone, LoginField::Code],
        }
    }

    /// The tab `delta` positions away, wrapping at both ends.
    fn shift(self, delta: isize) -> Self {
        let len = Self::ALL.len() as isize;
        Self::ALL[(self.index() as isize + delta).rem_euclid(len) as usize]
    }
}

/// One input of the form. Two of these are on screen at a time; which two is the method's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginField {
    #[default]
    Account,
    Password,
    Phone,
    Code,
}

impl LoginField {
    /// Whether the value must not be drawn. Only the password box is masked.
    pub const fn masked(self) -> bool {
        matches!(self, Self::Password)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LoginState {
    pub loading: bool,
    pub error: Option<String>,
    pub qr_url: String,
    pub qr_key: String,
    pub qr_status_text: String,
    /// Rendered QR lines (`Dense1x2`) keyed by the url they were encoded from,
    /// so the CPU-heavy QR encoding only runs when `qr_url` changes instead of
    /// on every frame. `None` means the current `qr_url` has not been encoded yet.
    pub qr_cache: Option<(String, Vec<String>)>,
    /// The method tab the box is showing.
    pub method: LoginMethod,
    /// The input the caret is in, among `method.fields()`.
    pub focus: LoginField,
    pub account: TextInput,
    pub password: TextInput,
    pub phone: TextInput,
    pub code: TextInput,
    /// A line for something that went *right* (the code went out, the check-in answered),
    /// drawn above the action row in accent. `error` stays what it was: a failure, in red.
    pub notice: Option<String>,
}

impl LoginState {
    /// Show `method` with a clean slate. The commands and the tab strip both come through
    /// here, so the error line of an earlier attempt does not greet the next one.
    pub fn open(&mut self, method: LoginMethod) {
        self.method = method;
        self.loading = false;
        self.error = None;
        self.notice = None;
        self.focus = method.fields().first().copied().unwrap_or_default();
    }

    /// `:signin <账号> <密码>`: the password form with both boxes filled.
    pub fn open_password(&mut self, account: &str, password: &str) {
        self.open(LoginMethod::Password);
        self.fill(LoginField::Account, account);
        self.fill(LoginField::Password, password);
        // The caret goes where there is still something to type; with both filled it stays on
        // the password box, which is the one `Enter` submits from.
        self.focus = if account.is_empty() {
            LoginField::Account
        } else {
            LoginField::Password
        };
    }

    /// `:sms <手机号>`: the code form with the number filled. The caret stays on the number
    /// box, because sending the code is the step that has not run yet.
    pub fn open_sms(&mut self, phone: &str) {
        self.open(LoginMethod::Sms);
        self.fill(LoginField::Phone, phone);
        self.focus = LoginField::Phone;
    }

    /// `:smslogin <手机号> <验证码>`: both boxes filled, ready for `Enter`.
    pub fn open_sms_login(&mut self, phone: &str, code: &str) {
        self.open(LoginMethod::Sms);
        self.fill(LoginField::Phone, phone);
        self.fill(LoginField::Code, code);
        self.focus = LoginField::Code;
    }

    /// Move the tab `delta` positions, keeping the form's contents.
    pub fn select_method(&mut self, delta: isize) {
        let next = self.method.shift(delta);
        self.open(next);
    }

    /// Move the caret to the next input of the current method, wrapping.
    pub fn focus_field(&mut self, delta: isize) {
        let fields = self.method.fields();
        if fields.is_empty() {
            return;
        }
        let current = fields.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(fields.len() as isize) as usize;
        self.focus = fields[next];
    }

    pub fn input(&self, field: LoginField) -> &TextInput {
        match field {
            LoginField::Account => &self.account,
            LoginField::Password => &self.password,
            LoginField::Phone => &self.phone,
            LoginField::Code => &self.code,
        }
    }

    pub fn input_mut(&mut self, field: LoginField) -> &mut TextInput {
        match field {
            LoginField::Account => &mut self.account,
            LoginField::Password => &mut self.password,
            LoginField::Phone => &mut self.phone,
            LoginField::Code => &mut self.code,
        }
    }

    /// The input the caret is in, or `None` when the method has no form (QR, check-in).
    pub fn focused_input_mut(&mut self) -> Option<&mut TextInput> {
        let field = self.focus;
        if self.method.fields().contains(&field) {
            Some(self.input_mut(field))
        } else {
            None
        }
    }

    /// The value of one input, which is what the requests are built from.
    pub fn value(&self, field: LoginField) -> String {
        self.input(field).value.clone()
    }

    /// Replace one input's contents, caret at the end.
    pub fn fill(&mut self, field: LoginField, text: &str) {
        set_field(self.input_mut(field), text);
    }
}

/// Replace a field's contents. `TextInput` keeps its caret private, so the text is typed in
/// rather than assigned: that leaves the caret at the end, which is where a value filled from
/// a command (or echoed back from the server) belongs.
fn set_field(input: &mut TextInput, text: &str) {
    *input = TextInput::new();
    for ch in text.chars() {
        input.enter_char(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tab_strip_wraps_in_both_directions() {
        let mut state = LoginState::default();
        for _ in 0..LoginMethod::ALL.len() {
            state.select_method(1);
        }
        assert_eq!(state.method, LoginMethod::Qr, "转一圈应回到起点");
        state.select_method(-1);
        assert_eq!(state.method, LoginMethod::Sign, "反向应回到最后一个");
    }

    #[test]
    fn the_commands_prefill_the_form_they_open() {
        // What `:signin a b` / `:sms p` / `:smslogin p c` promise: land on the page with the
        // method selected *and* the arguments already in the fields.
        let mut state = LoginState::default();
        state.open_password("someone@example.com", "hunter2");
        assert_eq!(state.method, LoginMethod::Password);
        assert_eq!(state.value(LoginField::Account), "someone@example.com");
        assert_eq!(state.value(LoginField::Password), "hunter2");

        let mut state = LoginState::default();
        state.open_sms("13800000000");
        assert_eq!(state.method, LoginMethod::Sms);
        assert_eq!(state.value(LoginField::Phone), "13800000000");

        let mut state = LoginState::default();
        state.open_sms_login("13800000000", "246810");
        assert_eq!(state.method, LoginMethod::Sms);
        assert_eq!(state.value(LoginField::Phone), "13800000000");
        assert_eq!(state.value(LoginField::Code), "246810");
    }

    #[test]
    fn only_the_password_is_masked() {
        assert!(LoginField::Password.masked());
        assert!(!LoginField::Account.masked());
    }
}
