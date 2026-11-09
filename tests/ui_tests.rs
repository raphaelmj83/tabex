//! Testes de interface com egui_kittest (árvore de acessibilidade, sem GPU).

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::path::PathBuf;
use tabex::app::{Action, TabexApp};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tabex_ui_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Pasta B")).unwrap();
    std::fs::create_dir_all(dir.join("Pasta A")).unwrap();
    std::fs::write(dir.join("relatorio.txt"), b"conteudo").unwrap();
    std::fs::write(dir.join("foto.jpg"), b"").unwrap();
    dir
}

fn harness(dir: &std::path::Path) -> Harness<'static, TabexApp> {
    let app = TabexApp::for_test(dir.to_path_buf());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_max_steps(400)
        .build_ui_state(|ui, app: &mut TabexApp| app.frame_for_test(ui), app);
    // Espera a listagem em segundo plano chegar.
    for _ in 0..200 {
        h.step();
        if !h.state().session.active_tab().listing.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    h
}

#[test]
fn lists_folder_contents_and_status_count() {
    let dir = temp_dir("list");
    let h = harness(&dir);
    h.get_by_label("relatorio.txt");
    h.get_by_label("foto.jpg");
    h.get_by_label("Pasta A");
    h.get_by_label_contains("4 itens");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_t_opens_new_tab_and_ctrl_w_closes_it() {
    let dir = temp_dir("tabs");
    let mut h = harness(&dir);
    assert_eq!(h.state().session.groups[0].tabs.len(), 1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::T);
    h.run_steps(3);
    assert_eq!(h.state().session.groups[0].tabs.len(), 2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::W);
    h.run_steps(3);
    assert_eq!(h.state().session.groups[0].tabs.len(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn new_window_dialog_creates_window_on_enter() {
    let dir = temp_dir("windows");
    let mut h = harness(&dir);
    assert_eq!(h.state().session.windows.len(), 1);
    let ctrl_alt = egui::Modifiers {
        alt: true,
        ctrl: false,
        shift: false,
        mac_cmd: false,
        command: true,
    };
    h.key_press_modifiers(ctrl_alt, egui::Key::T);
    h.run_steps(3);
    assert!(
        matches!(h.state().dialog, tabex::ui::dialogs::Dialog::Window { .. }),
        "o atalho Ctrl+Alt+T deve abrir o diálogo de nova janela"
    );
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(h.state().session.windows.len(), 2);
    assert_eq!(h.state().session.active_window, 1);
    // A barra de janelas mostra o nome da nova janela.
    assert!(h.get_all_by_label_contains("Janela 2").count() >= 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clicking_a_row_selects_it_and_filter_narrows_list() {
    let dir = temp_dir("select");
    let mut h = harness(&dir);
    h.get_by_label("relatorio.txt").click();
    h.run_steps(3);
    assert_eq!(h.state().session.active_tab().selected.len(), 1);
    h.get_by_label_contains("1 item selecionado");

    // Filtro instantâneo pela caixa de pesquisa.
    h.state_mut().pending.push(Action::FocusSearch);
    h.run_steps(2);
    h.state_mut().session.active_tab_mut().search_text = "*.jpg".into();
    h.state_mut().session.active_tab_mut().rebuild_rows();
    h.run_steps(2);
    h.get_by_label("foto.jpg");
    assert!(h.query_by_label("relatorio.txt").is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn group_and_window_actions_update_model() {
    let dir = temp_dir("groups");
    let mut h = harness(&dir);
    h.state_mut().pending.push(Action::OpenInNewGroup(dir.join("Pasta A")));
    h.run_steps(3);
    assert_eq!(h.state().session.groups.len(), 2);
    assert!(h.get_all_by_label_contains("Grupo 2").count() >= 1);
    h.state_mut().pending.push(Action::OpenInNewWindow(dir.join("Pasta B")));
    h.run_steps(3);
    assert_eq!(h.state().session.windows.len(), 2);
    assert_eq!(h.state().session.active_tab().path, dir.join("Pasta B"));
    std::fs::remove_dir_all(&dir).unwrap();
}
