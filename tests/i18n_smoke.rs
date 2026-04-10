use std::sync::Once;

use anyhow::Result;
use es_fluent::unic_langid::LanguageIdentifier;
use torii as _;

static I18N_INIT: Once = Once::new();

fn init_i18n() {
    I18N_INIT.call_once(es_fluent_manager_embedded::init);
}

#[test]
fn fluent_resolves_known_keys_for_en_and_zh_cn() -> Result<()> {
    init_i18n();

    let en: LanguageIdentifier = "en".parse()?;
    es_fluent_manager_embedded::select_language(en.clone());
    es_fluent::select_language(&en);
    assert_eq!(
        es_fluent::localize("form_page_title", None),
        "Create Account"
    );
    assert_eq!(es_fluent::localize("form_submit", None), "Create Account");

    let zh_cn: LanguageIdentifier = "zh-CN".parse()?;
    es_fluent_manager_embedded::select_language(zh_cn.clone());
    es_fluent::select_language(&zh_cn);
    assert_eq!(es_fluent::localize("form_page_title", None), "创建账户");
    assert_eq!(es_fluent::localize("form_submit", None), "创建账户");

    Ok(())
}
