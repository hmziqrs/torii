use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, WindowExt as _, h_flex, v_flex,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    form::{field, v_form},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
};
use gpui_form::GpuiForm;
use koruma::Koruma;
use koruma_collection::{
    collection::NonEmptyValidation,
    format::{EmailValidation, UrlValidation},
};

// ---------------------------------------------------------------------------
// Form model with derive macros
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, GpuiForm, Koruma)]
#[gpui_form(koruma)]
pub struct RegistrationForm {
    #[gpui_form(component(input))]
    #[koruma(NonEmptyValidation::<_>)]
    pub name: String,

    #[gpui_form(component(input))]
    #[koruma(EmailValidation::<_>)]
    pub email: String,

    #[gpui_form(component(input))]
    #[koruma(NonEmptyValidation::<_>)]
    pub password: String,

    #[gpui_form(component(input))]
    #[koruma(NonEmptyValidation::<_>)]
    pub phone: String,

    #[gpui_form(component(input))]
    #[koruma(UrlValidation::<_>)]
    pub website: String,
}

// ---------------------------------------------------------------------------
// Form page
// ---------------------------------------------------------------------------

pub struct FormPage {
    current_data: RegistrationFormFormValueHolder,
    fields: RegistrationFormFormFields,
    agree_terms: bool,
    submitted: bool,
    touched: bool,
    _subscriptions: Vec<Subscription>,
}

impl FormPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current_data = RegistrationFormFormValueHolder::default();

        let name_input = cx.new(|cx| RegistrationFormFormComponents::name_input(window, cx));
        let email_input = cx.new(|cx| RegistrationFormFormComponents::email_input(window, cx));
        let password_input = cx.new(|cx| RegistrationFormFormComponents::password_input(window, cx));
        let phone_input = cx.new(|cx| RegistrationFormFormComponents::phone_input(window, cx));
        let website_input = cx.new(|cx| RegistrationFormFormComponents::website_input(window, cx));

        let _subscriptions = vec![
            cx.subscribe(&name_input, |this: &mut FormPage, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value();
                    this.current_data.name = if text.is_empty() { None } else { Some(text.to_string()) };
                }
            }),
            cx.subscribe(&email_input, |this: &mut FormPage, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value();
                    this.current_data.email = if text.is_empty() { None } else { Some(text.to_string()) };
                }
            }),
            cx.subscribe(&password_input, |this: &mut FormPage, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value();
                    this.current_data.password = if text.is_empty() { None } else { Some(text.to_string()) };
                }
            }),
            cx.subscribe(&phone_input, |this: &mut FormPage, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value();
                    this.current_data.phone = if text.is_empty() { None } else { Some(text.to_string()) };
                }
            }),
            cx.subscribe(&website_input, |this: &mut FormPage, state: Entity<InputState>, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value();
                    this.current_data.website = if text.is_empty() { None } else { Some(text.to_string()) };
                }
            }),
        ];

        Self {
            current_data,
            fields: RegistrationFormFormFields {
                name_input,
                email_input,
                password_input,
                phone_input,
                website_input,
            },
            agree_terms: false,
            submitted: false,
            touched: false,
            _subscriptions,
        }
    }

    fn on_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.current_data = RegistrationFormFormValueHolder::default();
        self.fields.name_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.fields.email_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.fields.password_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.fields.phone_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.fields.website_input.update(cx, |s, cx| s.set_value("", window, cx));
        self.agree_terms = false;
        self.submitted = false;
        self.touched = false;
        cx.notify();
    }
}

impl Render for FormPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let validation_errors = if self.touched {
            self.current_data.validate().err()
        } else {
            None
        };

        let error_for = |field_name: &str| -> Option<String> {
            validation_errors.as_ref().and_then(|e| {
                let errs: Vec<String> = match field_name {
                    "name" => e.name().all().iter().map(|v| format!("{:?}", v)).collect(),
                    "email" => e.email().all().iter().map(|v| format!("{:?}", v)).collect(),
                    "password" => e.password().all().iter().map(|v| format!("{:?}", v)).collect(),
                    "phone" => e.phone().all().iter().map(|v| format!("{:?}", v)).collect(),
                    "website" => e.website().all().iter().map(|v| format!("{:?}", v)).collect(),
                    _ => Vec::new(),
                };
                if errs.is_empty() { None } else { Some(errs.join(", ")) }
            })
        };

        let danger = cx.theme().danger;

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
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Powered by gpui-form + koruma validation"),
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
            .child(
                v_form()
                    .label_width(px(160.))
                    // Name
                    .child(
                        field()
                            .label("Full Name")
                            .required(true)
                            .description_fn({
                                let error = error_for("name");
                                move |_, _| {
                                    div().flex().flex_col().gap_1()
                                        .child(div().child("Enter your full name."))
                                        .when_some(error.clone(), |el, err| {
                                            el.child(div().text_color(danger).text_xs().child(err))
                                        })
                                }
                            })
                            .child(Input::new(&self.fields.name_input)),
                    )
                    // Email
                    .child(
                        field()
                            .label("Email")
                            .required(true)
                            .description_fn({
                                let error = error_for("email");
                                move |_, _| {
                                    div().flex().flex_col().gap_1()
                                        .child(div().child("We'll never share your email with anyone else."))
                                        .when_some(error.clone(), |el, err| {
                                            el.child(div().text_color(danger).text_xs().child(err))
                                        })
                                }
                            })
                            .child(Input::new(&self.fields.email_input)),
                    )
                    // Password
                    .child(
                        field()
                            .label("Password")
                            .required(true)
                            .description_fn({
                                let error = error_for("password");
                                move |_, _| {
                                    div().flex().flex_col().gap_1()
                                        .child(div().child("Choose a strong password."))
                                        .when_some(error.clone(), |el, err| {
                                            el.child(div().text_color(danger).text_xs().child(err))
                                        })
                                }
                            })
                            .child(Input::new(&self.fields.password_input)),
                    )
                    // Phone
                    .child(
                        field()
                            .label("Phone")
                            .required(true)
                            .description_fn({
                                let error = error_for("phone");
                                move |_, _| {
                                    div().flex().flex_col().gap_1()
                                        .child(div().child("Format: (555) 123-4567"))
                                        .when_some(error.clone(), |el, err| {
                                            el.child(div().text_color(danger).text_xs().child(err))
                                        })
                                }
                            })
                            .child(Input::new(&self.fields.phone_input)),
                    )
                    // Website (optional)
                    .child(
                        field()
                            .label("Website")
                            .description_fn({
                                let error = error_for("website");
                                move |_, _| {
                                    div().flex().flex_col().gap_1()
                                        .child(div().child("Optional. Your personal or company site."))
                                        .when_some(error.clone(), |el, err| {
                                            el.child(div().text_color(danger).text_xs().child(err))
                                        })
                                }
                            })
                            .child(Input::new(&self.fields.website_input)),
                    )
                    // Terms
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
                    // Actions
                    .child(
                        field().label_indent(false).child(
                            h_flex()
                                .gap_3()
                                .pt_2()
                                .child(
                                    Button::new("submit")
                                        .primary()
                                        .label("Create Account")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.touched = true;
                                            let valid = this.current_data.validate().is_ok();
                                            if valid && this.agree_terms {
                                                this.submitted = true;
                                                window.push_notification("Form submitted successfully!", cx);
                                            } else if valid && !this.agree_terms {
                                                window.push_notification("You must agree to the terms and conditions", cx);
                                            } else {
                                                window.push_notification("Please fix the errors in the form", cx);
                                            }
                                            cx.notify();
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
                        ),
                    ),
            )
    }
}
