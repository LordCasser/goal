mod common;

use planner_lib::{db, service::settings};

#[test]
fn relation_lines_default_to_on_and_persist_across_reopen() {
    let f = common::TestDb::open();
    assert!(settings::get(&f.db).unwrap().show_relation_lines);

    settings::set_show_relation_lines(&f.db, false).unwrap();
    assert!(!settings::get(&f.db).unwrap().show_relation_lines);

    let path = f.db.path().unwrap();
    drop(f.db);
    let reopened = db::open_at(&path).unwrap();
    assert!(!settings::get(&reopened).unwrap().show_relation_lines);

    settings::set_show_relation_lines(&reopened, true).unwrap();
    assert!(settings::get(&reopened).unwrap().show_relation_lines);
}
