mod common;
use planner_lib::{db, i18n::Locale, service::settings};

#[test]
fn locale_persists_across_database_reopen_and_invalid_writes_keep_previous_value() {
    let f = common::TestDb::open();
    let first = settings::get(&f.db).unwrap().locale;
    assert!(matches!(first, Locale::En | Locale::ZhCn));
    settings::set_locale(&f.db, "zh-CN".into()).unwrap();
    assert_eq!(settings::get(&f.db).unwrap().locale, Locale::ZhCn);
    for invalid in ["fr", "en-US", "", "zh", "ZH-CN"] {
        assert!(settings::set_locale(&f.db, invalid.into()).is_err());
        assert_eq!(settings::get(&f.db).unwrap().locale, Locale::ZhCn);
    }
    assert!(settings::set_app_flag(&f.db, "locale".into(), "invalid".into()).is_err());
    settings::set_locale(&f.db, "en".into()).unwrap();
    let path = f.db.path().unwrap();
    drop(f.db);
    let reopened = db::open_at(&path).unwrap();
    assert_eq!(settings::get(&reopened).unwrap().locale, Locale::En);
}
