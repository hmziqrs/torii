use super::*;
use crate::domain::response::ResponseHeaderRow;

pub(crate) struct HeadersTableDelegate {
    rows: Vec<ResponseHeaderRow>,
    columns: Vec<Column>,
}

impl HeadersTableDelegate {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new(
                    "name",
                    es_fluent::localize("request_tab_response_headers_col_name", None),
                )
                .width(px(200.))
                .resizable(true)
                .movable(false),
                Column::new(
                    "value",
                    es_fluent::localize("request_tab_response_headers_col_value", None),
                )
                .width(px(500.))
                .resizable(true)
                .movable(false),
            ],
        }
    }

    pub(crate) fn set_rows(&mut self, rows: Vec<ResponseHeaderRow>) {
        self.rows = rows;
    }
}

impl TableDelegate for HeadersTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            0 => div()
                .font_family("monospace")
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(row.name.clone())
                .into_any_element(),
            _ => div()
                .font_family("monospace")
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(row.value.clone())
                .into_any_element(),
        }
    }
}

pub(crate) struct CookiesTableDelegate {
    rows: Vec<CookieRow>,
    columns: Vec<Column>,
}

impl CookiesTableDelegate {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new(
                    "name",
                    es_fluent::localize("request_tab_cookies_col_name", None),
                )
                .width(px(120.))
                .resizable(true)
                .movable(false),
                Column::new(
                    "value",
                    es_fluent::localize("request_tab_cookies_col_value", None),
                )
                .width(px(150.))
                .resizable(true)
                .movable(false),
                Column::new(
                    "domain",
                    es_fluent::localize("request_tab_cookies_col_domain", None),
                )
                .width(px(120.))
                .resizable(true)
                .movable(false),
                Column::new(
                    "path",
                    es_fluent::localize("request_tab_cookies_col_path", None),
                )
                .width(px(80.))
                .resizable(true)
                .movable(false),
                Column::new(
                    "expires",
                    es_fluent::localize("request_tab_cookies_col_expires", None),
                )
                .width(px(120.))
                .resizable(true)
                .movable(false),
                Column::new(
                    "secure",
                    es_fluent::localize("request_tab_cookies_col_secure", None),
                )
                .width(px(60.))
                .resizable(false)
                .movable(false),
                Column::new(
                    "httponly",
                    es_fluent::localize("request_tab_cookies_col_httponly", None),
                )
                .width(px(70.))
                .resizable(false)
                .movable(false),
                Column::new(
                    "samesite",
                    es_fluent::localize("request_tab_cookies_col_samesite", None),
                )
                .width(px(80.))
                .resizable(true)
                .movable(false),
            ],
        }
    }

    pub(crate) fn set_rows(&mut self, rows: Vec<CookieRow>) {
        self.rows = rows;
    }
}

impl TableDelegate for CookiesTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let cookie = &self.rows[row_ix];
        match col_ix {
            0 => div()
                .font_weight(FontWeight::MEDIUM)
                .child(cookie.name.clone())
                .into_any_element(),
            1 => div().child(cookie.value_preview.clone()).into_any_element(),
            2 => div()
                .child(cookie.domain.clone().unwrap_or_else(|| "—".to_string()))
                .into_any_element(),
            3 => div()
                .child(cookie.path.clone().unwrap_or_else(|| "—".to_string()))
                .into_any_element(),
            4 => div()
                .child(
                    cookie
                        .expires_or_max_age
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                )
                .into_any_element(),
            5 => div()
                .child(if cookie.secure { "true" } else { "false" })
                .into_any_element(),
            6 => div()
                .child(if cookie.http_only { "true" } else { "false" })
                .into_any_element(),
            _ => div()
                .child(cookie.same_site.clone().unwrap_or_else(|| "—".to_string()))
                .into_any_element(),
        }
    }
}

pub(crate) struct TimingRow {
    pub(crate) phase: String,
    pub(crate) value: String,
}

pub(crate) struct TimingTableDelegate {
    rows: Vec<TimingRow>,
    columns: Vec<Column>,
}

impl TimingTableDelegate {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new(
                    "phase",
                    es_fluent::localize("request_tab_response_timing_col_phase", None),
                )
                .width(px(200.))
                .resizable(false)
                .movable(false),
                Column::new(
                    "value",
                    es_fluent::localize("request_tab_response_timing_col_value", None),
                )
                .width(px(400.))
                .resizable(true)
                .movable(false),
            ],
        }
    }

    pub(crate) fn set_rows(&mut self, rows: Vec<TimingRow>) {
        self.rows = rows;
    }
}

impl TableDelegate for TimingTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        match col_ix {
            0 => div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(row.phase.clone())
                .into_any_element(),
            _ => div()
                .text_sm()
                .font_family("monospace")
                .child(row.value.clone())
                .into_any_element(),
        }
    }
}
