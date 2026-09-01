use qa_policy::QaConfig;
use qa_rules::analyze;
use qa_syntax::discover;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

fn workspace(source: &str) -> PathBuf {
    let id = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("urqa-rules-{}-{id}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), source).unwrap();
    root
}

#[test]
fn phase8_state_contracts_are_detected() {
    let root = workspace(
        r#"
#[qa_attr::critical_state]
#[qa_attr::state_machine]
enum Session { Init, Ready }
#[qa_attr::critical_state]
fn step(s: Session) -> Result<Session, ()> { match s { Session::Init => Ok(Session::Ready), _ => panic!("bad") } }
"#,
    );
    let output = analyze(&discover(&root), &QaConfig::default());
    let ids = output.findings.iter().map(|finding| finding.rule_id.as_str()).collect::<Vec<_>>();
    assert!(ids.contains(&"QA-STATE-001"));
    assert!(ids.contains(&"QA-STATE-002"));
    fs::remove_dir_all(root).expect("remove phase 8 fixture workspace");
}

#[test]
fn phase9_async_and_concurrency_contracts_are_detected() {
    let root = workspace(
        r#"
#[qa_attr::critical_async]
async fn work() { std::thread::sleep(std::time::Duration::from_millis(1)); tokio::spawn(async {}); do_it().await; }
fn do_it() {}
"#,
    );
    let output = analyze(&discover(&root), &QaConfig::default());
    let ids = output.findings.iter().map(|finding| finding.rule_id.as_str()).collect::<Vec<_>>();
    assert!(ids.contains(&"QA-ASYNC-001"));
    assert!(ids.contains(&"QA-ASYNC-003"));
    fs::remove_dir_all(root).expect("remove phase 9 fixture workspace");
}

#[test]
fn phase10_secret_contracts_are_detected() {
    let root = workspace(
        r#"
#[derive(Debug)]
#[qa_attr::secret]
struct Seed([u8;32]);
fn show(secret_seed: Seed) { println!("{:?}", secret_seed); }
"#,
    );
    let output = analyze(&discover(&root), &QaConfig::default());
    let ids = output.findings.iter().map(|finding| finding.rule_id.as_str()).collect::<Vec<_>>();
    assert!(ids.contains(&"QA-ERR-002"));
    assert!(ids.contains(&"QA-SECRET-002"));
    fs::remove_dir_all(root).expect("remove phase 10 fixture workspace");
}

#[test]
fn phase15_build_layout_and_ffi_contracts_are_detected() {
    let root = workspace(
        r#"
#[qa_attr::critical_layout]
struct Wire { a: u32 }
pub unsafe extern "C" fn exported(v: Vec<u8>) { panic!("x"); }
"#,
    );
    fs::write(
        root.join("build.rs"),
        "fn main(){ let _ = std::net::TcpStream::connect(\"example:1\"); }",
    )
    .unwrap();
    let ws = discover(&root);
    assert!(
        ws.types.iter().any(|ty| {
            ty.name == "Wire" && ty.attributes.iter().any(|attr| attr.contains("critical_layout"))
        }),
        "phase 15 fixture discovery lost the critical_layout Wire type"
    );
    let output = analyze(&ws, &QaConfig::default());
    let ids = output.findings.iter().map(|f| f.rule_id.as_str()).collect::<Vec<_>>();
    assert!(ids.contains(&"QA-BUILD-003"));
    assert!(ids.contains(&"QA-LAYOUT-001"));
    assert!(ids.contains(&"QA-FFI-002"));
    fs::remove_dir_all(root).expect("remove phase 8-15 fixture workspace");
}
