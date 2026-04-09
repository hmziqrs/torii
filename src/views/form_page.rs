use std::collections::HashMap;
use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, WindowExt as _, h_flex, v_flex,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    form::{field, v_form},
    input::{Input, InputState},
    scroll::ScrollableElement as _,
};

// ---------------------------------------------------------------------------
// Validation state
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct FieldValidation {
    error: Option<String>,
    touched: bool,
}

// ---------------------------------------------------------------------------
// Form page
// ---------------------------------------------------------------------------

pub struct FormPage {
    name_input: Entity<InputState>,
    email_input: Entity<InputState>,
    password_input: Entity<InputState>,
    confirm_password_input: Entity<InputState>,
    phone_input: Entity<InputState>,
    website_input: Entity<InputState>,
    agree_terms: bool,
    submitted: bool,
    validation: HashMap<&'static str, FieldValidation>,
}

impl FormPage {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Enter your full name")
        });
        let email_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("you@example.com")
                .pattern(regex::Regex::new(r"^[a-zA-Z0-9@._\-+]*$").unwrap())
        });
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("At least 8 characters")
                .masked(true)
        });
        let confirm_password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Re-enter your password")
                .masked(true)
        });
        let phone_input = cx.new(|cx| {
            InputState::new(window, cx)
                .mask_pattern("(999) 999-9999")
                .placeholder("Phone number")
        });
        let website_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://example.com")
                .pattern(regex::Regex::new(r"^[a-zA-Z0-9:/.\-_]*$").unwrap())
        });

        let mut validation = HashMap::new();
        validation.insert("name", FieldValidation::default());
        validation.insert("email", FieldValidation::default());
        validation.insert("password", FieldValidation::default());
        validation.insert("confirm_password", FieldValidation::default());
        validation.insert("phone", FieldValidation::default());

        Self {
            name_input,
            email_input,
            password_input,
            confirm_password_input,
            phone_input,
            website_input,
            agree_terms: false,
            submitted: false,
            validation,
        }
    }

    fn validate_all(&mut self, cx: &App) -> bool {
        let name = self.name_input.read(cx).value().trim().to_string();
        let email = self.email_input.read(cx).value().trim().to_string();
        let password = self.password_input.read(cx).value().to_string();
        let confirm = self.confirm_password_input.read(cx).value().to_string();
        let phone = self.phone_input.read(cx).value().trim().to_string();

        let mut valid = true;

        // Name: required, min 2 chars
        let err = if name.is_empty() {
            Some("Name is required".into())
        } else if name.len() < 2 {
            Some("Name must be at least 2 characters".into())
        } else {
            None
        };
        self.validation.get_mut("name").unwrap().error = err.clone();
        self.validation.get_mut("name").unwrap().touched = true;
        if err.is_some() { valid = false; }

        // Email: required, valid format
        let email_re = regex::Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap();
        let err = if email.is_empty() {
            Some("Email is required".into())
        } else if !email_re.is_match(&email) {
            Some("Please enter a valid email address".into())
        } else {
            None
        };
        self.validation.get_mut("email").unwrap().error = err.clone();
        self.validation.get_mut("email").unwrap().touched = true;
        if err.is_some() { valid = false; }

        // Password: required, min 8 chars, must contain number + letter
        let has_letter = password.chars().any(|c| c.is_ascii_alphabetic());
        let has_number = password.chars().any(|c| c.is_ascii_digit());
        let err = if password.is_empty() {
            Some("Password is required".into())
        } else if password.len() < 8 {
            Some("Password must be at least 8 characters".into())
        } else if !has_letter || !has_number {
            Some("Must contain both letters and numbers".into())
        } else {
            None
        };
        self.validation.get_mut("password").unwrap().error = err.clone();
        self.validation.get_mut("password").unwrap().touched = true;
        if err.is_some() { valid = false; }

        // Confirm password: must match
        let err = if confirm.is_empty() {
            Some("Please confirm your password".into())
        } else if confirm != password {
            Some("Passwords do not match".into())
        } else {
            None
        };
        self.validation.get_mut("confirm_password").unwrap().error = err.clone();
        self.validation.get_mut("confirm_password").unwrap().touched = true;
        if err.is_some() { valid = false; }

        // Phone: required
        let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        let err = if digits.is_empty() {
            Some("Phone number is required".into())
        } else if digits.len() < 10 {
            Some("Please enter a complete phone number".into())
        } else {
            None
        };
        self.validation.get_mut("phone").unwrap().error = err.clone();
        self.validation.get_mut("phone").unwrap().touched = true;
        if err.is_some() { valid = false; }

        valid
    }

    fn on_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let valid = self.validate_all(cx);
        if valid && self.agree_terms {
            self.submitted = true;
            window.push_notification("Form submitted successfully!", cx);
        } else if valid && !self.agree_terms {
            window.push_notification("You must agree to the terms and conditions", cx);
        } else {
            window.push_notification("Please fix the errors in the form", cx);
        }
        cx.notify();
    }

    fn on_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.name_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.email_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.password_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.confirm_password_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.phone_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.website_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.agree_terms = false;
        self.submitted = false;
        for v in self.validation.values_mut() {
            v.error = None;
            v.touched = false;
        }
        cx.notify();
    }

    fn error_element(&self, field_name: &'static str, cx: &App) -> impl IntoElement {
        let v = self.validation.get(field_name);
        match v.and_then(|v| v.error.clone()) {
            Some(err) => div()
                .text_xs()
                .text_color(cx.theme().danger)
                .pt_1()
                .child(err)
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    fn border_color(&self, field_name: &'static str, cx: &App) -> Hsla {
        match self.validation.get(field_name) {
            Some(v) if v.touched && v.error.is_some() => cx.theme().danger,
            _ => cx.theme().border,
        }
    }
}

impl Render for FormPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let name_border = self.border_color("name", cx);
        let email_border = self.border_color("email", cx);
        let pwd_border = self.border_color("password", cx);
        let confirm_border = self.border_color("confirm_password", cx);
        let phone_border = self.border_color("phone", cx);

        v_flex()
            .size_full()
            .overflow_y_scrollbar()
            .p_6()
            .gap_4()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Create Account"),
            )
            .when(self.submitted, |this| {
                this.child(
                    div()
                        .p_3()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().success.opacity(0.1))
                        .border_1()
                        .border_color(cx.theme().success)
                        .text_color(cx.theme().success)
                        .child("Account created successfully! Check your email for verification."),
                )
            })
            // ── Personal Info ──
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("PERSONAL INFORMATION"),
            )
            .child(
                v_form()
                    .label_width(px(160.))
                    // Name
                    .child(
                        field()
                            .label("Full Name")
                            .required(true)
                            .child(
                                div()
                                    .border_1()
                                    .border_color(name_border)
                                    .rounded(cx.theme().radius)
                                    .child(Input::new(&self.name_input)),
                            ),
                    )
                    .child(
                        field().child(self.error_element("name", cx)),
                    )
                    // Email
                    .child(
                        field()
                            .label("Email")
                            .required(true)
                            .description("We'll never share your email with anyone else.")
                            .child(
                                div()
                                    .border_1()
                                    .border_color(email_border)
                                    .rounded(cx.theme().radius)
                                    .child(Input::new(&self.email_input)),
                            ),
                    )
                    .child(
                        field().child(self.error_element("email", cx)),
                    )
                    // Phone
                    .child(
                        field()
                            .label("Phone")
                            .required(true)
                            .description("Format: (555) 123-4567")
                            .child(
                                div()
                                    .border_1()
                                    .border_color(phone_border)
                                    .rounded(cx.theme().radius)
                                    .child(Input::new(&self.phone_input)),
                            ),
                    )
                    .child(
                        field().child(self.error_element("phone", cx)),
                    )
                    // Website (optional)
                    .child(
                        field()
                            .label("Website")
                            .description("Optional. Your personal or company site.")
                            .child(
                                div()
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .rounded(cx.theme().radius)
                                    .child(Input::new(&self.website_input)),
                            ),
                    ),
            )
            // ── Security ──
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child("SECURITY"),
            )
            .child(
                v_form()
                    .label_width(px(160.))
                    // Password
                    .child(
                        field()
                            .label("Password")
                            .required(true)
                            .description("At least 8 characters with letters and numbers.")
                            .child(
                                div()
                                    .border_1()
                                    .border_color(pwd_border)
                                    .rounded(cx.theme().radius)
                                    .child(Input::new(&self.password_input)),
                            ),
                    )
                    .child(
                        field().child(self.error_element("password", cx)),
                    )
                    // Confirm Password
                    .child(
                        field()
                            .label("Confirm Password")
                            .required(true)
                            .child(
                                div()
                                    .border_1()
                                    .border_color(confirm_border)
                                    .rounded(cx.theme().radius)
                                    .child(Input::new(&self.confirm_password_input)),
                            ),
                    )
                    .child(
                        field().child(self.error_element("confirm_password", cx)),
                    ),
            )
            // ── Terms ──
            .child(
                field().label_indent(false).child(
                    Checkbox::new("agree-terms")
                        .label("I agree to the Terms and Conditions")
                        .checked(self.agree_terms)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.agree_terms = *checked;
                            cx.notify();
                        })),
                ),
            )
            // ── Actions ──
            .child(
                h_flex()
                    .gap_3()
                    .pt_2()
                    .child(
                        Button::new("submit")
                            .primary()
                            .label("Create Account")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_submit(window, cx);
                            })),
                    )
                    .child(
                        Button::new("reset")
                            .ghost()
                            .label("Reset")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_reset(window, cx);
                            })),
                    ),
            )
    }
}
