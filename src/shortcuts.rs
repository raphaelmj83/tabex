use crate::app::Action;
use egui::{Context, Event, Key, Modifiers};

const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: false,
    shift: true,
    mac_cmd: false,
    command: true,
};
const CMD_ALT: Modifiers = Modifiers {
    alt: true,
    ctrl: false,
    shift: false,
    mac_cmd: false,
    command: true,
};
const ALT: Modifiers = Modifiers::ALT;
const SHIFT: Modifiers = Modifiers::SHIFT;
const NONE: Modifiers = Modifiers::NONE;

/// Tabela de atalhos exibida em "Atalhos de teclado" (F1).
pub const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Abas e grupos",
        &[
            ("Ctrl+T", "Nova aba"),
            ("Ctrl+W", "Fechar aba"),
            ("Ctrl+Shift+T", "Reabrir aba fechada"),
            ("Ctrl+Tab / Ctrl+Shift+Tab", "Próxima / aba anterior"),
            ("Ctrl+PgDn / Ctrl+PgUp", "Próxima / aba anterior"),
            ("Ctrl+Shift+PgDn / PgUp", "Mover aba para a direita / esquerda"),
            ("Ctrl+1 … Ctrl+8, Ctrl+9", "Ir para a aba N / última aba do grupo"),
            ("Ctrl+G", "Novo grupo de abas"),
            ("Ctrl+Shift+G", "Renomear / cor do grupo atual"),
            ("Ctrl+Alt+→ / ←", "Próximo / grupo anterior"),
            ("Clique do meio na aba", "Fechar aba"),
            ("Arrastar aba", "Mover para outro grupo ou posição"),
        ],
    ),
    (
        "Navegação",
        &[
            ("Alt+← / Backspace", "Voltar"),
            ("Alt+→", "Avançar"),
            ("Alt+↑", "Subir um nível"),
            ("F5 / Ctrl+R", "Atualizar"),
            ("Ctrl+L / Alt+D / F4", "Editar barra de endereços"),
            ("Ctrl+E / Ctrl+F / F3", "Ir para a pesquisa"),
            ("Enter na pesquisa", "Pesquisar em subpastas"),
            ("Esc", "Limpar pesquisa / filtros / seleção"),
            ("Ctrl+B", "Mostrar/ocultar painel de navegação"),
            ("Ctrl+H", "Mostrar/ocultar itens ocultos"),
            ("Ctrl+D", "Fixar / desafixar pasta atual"),
            ("Digitar letras", "Pular para o item que começa com elas"),
        ],
    ),
    (
        "Arquivos",
        &[
            ("Enter / duplo clique", "Abrir"),
            ("Ctrl+duplo clique / clique do meio", "Abrir pasta em nova aba"),
            ("Ctrl+C / Ctrl+X / Ctrl+V", "Copiar / Recortar / Colar"),
            ("Delete", "Mover para a Lixeira"),
            ("Shift+Delete", "Excluir permanentemente"),
            ("F2", "Renomear"),
            ("Ctrl+Shift+N", "Nova pasta"),
            ("Ctrl+A", "Selecionar tudo"),
            ("Ctrl+Shift+C", "Copiar caminho"),
            ("Alt+Enter", "Propriedades"),
            ("Shift+↑/↓, Shift+clique", "Estender seleção"),
            ("Ctrl+clique", "Alternar item na seleção"),
            ("F1", "Esta janela"),
        ],
    ),
];

/// Lê o teclado e devolve as ações correspondentes.
/// `editing_address` evita que Esc/Enter da barra de endereços virem ações da lista.
pub fn collect(ctx: &Context, editing_address: bool) -> Vec<Action> {
    let mut out = Vec::new();
    let text_focused = ctx.memory(|m| m.focused().is_some());

    ctx.input_mut(|i| {
        // ---- globais (funcionam mesmo com um campo de texto em foco) ----
        if i.consume_key(CMD_SHIFT, Key::T) {
            out.push(Action::ReopenClosedTab);
        }
        if i.consume_key(CMD, Key::T) {
            out.push(Action::NewTab);
        }
        if i.consume_key(CMD, Key::W) {
            out.push(Action::CloseActiveTab);
        }
        if i.consume_key(CMD_SHIFT, Key::Tab) || i.consume_key(CMD, Key::PageUp) {
            out.push(Action::CycleTab(-1));
        }
        if i.consume_key(CMD, Key::Tab) || i.consume_key(CMD, Key::PageDown) {
            out.push(Action::CycleTab(1));
        }
        if i.consume_key(CMD_SHIFT, Key::PageDown) {
            out.push(Action::MoveTab(1));
        }
        if i.consume_key(CMD_SHIFT, Key::PageUp) {
            out.push(Action::MoveTab(-1));
        }
        let nums = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
        ];
        for (n, k) in nums.iter().enumerate() {
            if i.consume_key(CMD, *k) {
                out.push(Action::ActivateTabIndex(n));
            }
        }
        if i.consume_key(CMD, Key::Num9) {
            out.push(Action::ActivateTabIndex(usize::MAX));
        }
        if i.consume_key(CMD_SHIFT, Key::G) {
            out.push(Action::EditActiveGroup);
        }
        if i.consume_key(CMD, Key::G) {
            out.push(Action::NewGroup);
        }
        if i.consume_key(CMD_ALT, Key::ArrowRight) {
            out.push(Action::CycleGroup(1));
        }
        if i.consume_key(CMD_ALT, Key::ArrowLeft) {
            out.push(Action::CycleGroup(-1));
        }

        if i.consume_key(ALT, Key::ArrowLeft) {
            out.push(Action::Back);
        }
        if i.consume_key(ALT, Key::ArrowRight) {
            out.push(Action::Forward);
        }
        if i.consume_key(ALT, Key::ArrowUp) {
            out.push(Action::Up);
        }
        if i.consume_key(NONE, Key::F5) || i.consume_key(CMD, Key::R) {
            out.push(Action::Refresh);
        }
        if i.consume_key(CMD, Key::L) || i.consume_key(ALT, Key::D) || i.consume_key(NONE, Key::F4) {
            out.push(Action::FocusAddress);
        }
        if i.consume_key(CMD, Key::E) || i.consume_key(CMD, Key::F) || i.consume_key(NONE, Key::F3) {
            out.push(Action::FocusSearch);
        }
        if i.consume_key(CMD_SHIFT, Key::N) {
            out.push(Action::NewFolder);
        }
        if i.consume_key(CMD, Key::H) {
            out.push(Action::ToggleHidden);
        }
        if i.consume_key(CMD, Key::B) {
            out.push(Action::ToggleSidebar);
        }
        if i.consume_key(CMD_SHIFT, Key::C) {
            out.push(Action::CopyPath);
        }
        if i.consume_key(NONE, Key::F1) {
            out.push(Action::ShowShortcuts);
        }
        if i.consume_key(ALT, Key::Enter) {
            out.push(Action::Properties);
        }

        // ---- apenas quando nenhum campo de texto está em foco ----
        if text_focused || editing_address {
            return;
        }
        if i.consume_key(CMD, Key::D) {
            out.push(Action::TogglePinCurrent);
        }
        if i.consume_key(NONE, Key::Backspace) {
            out.push(Action::Back);
        }
        if i.consume_key(NONE, Key::Enter) {
            out.push(Action::OpenSelected);
        }
        if i.consume_key(NONE, Key::F2) {
            out.push(Action::Rename);
        }
        if i.consume_key(SHIFT, Key::Delete) {
            out.push(Action::Delete { permanent: true });
        }
        if i.consume_key(NONE, Key::Delete) {
            out.push(Action::Delete { permanent: false });
        }
        if i.consume_key(CMD, Key::C) {
            out.push(Action::Copy);
        }
        if i.consume_key(CMD, Key::X) {
            out.push(Action::Cut);
        }
        if i.consume_key(CMD, Key::V) {
            out.push(Action::Paste);
        }
        if i.consume_key(CMD, Key::A) {
            out.push(Action::SelectAll);
        }
        if i.consume_key(NONE, Key::Escape) {
            out.push(Action::Escape);
        }
        for (key, delta) in [
            (Key::ArrowDown, 1isize),
            (Key::ArrowUp, -1),
            (Key::PageDown, 12),
            (Key::PageUp, -12),
        ] {
            if i.consume_key(SHIFT, key) {
                out.push(Action::MoveCursor(delta, true));
            }
            if i.consume_key(NONE, key) {
                out.push(Action::MoveCursor(delta, false));
            }
        }
        if i.consume_key(SHIFT, Key::Home) {
            out.push(Action::CursorHome(true));
        }
        if i.consume_key(NONE, Key::Home) {
            out.push(Action::CursorHome(false));
        }
        if i.consume_key(SHIFT, Key::End) {
            out.push(Action::CursorEnd(true));
        }
        if i.consume_key(NONE, Key::End) {
            out.push(Action::CursorEnd(false));
        }
        // Digitação rápida: letras/números sem modificadores.
        if !i.modifiers.command && !i.modifiers.alt {
            for ev in &i.events {
                if let Event::Text(t) = ev {
                    if t.chars().all(|c| !c.is_control()) && !t.trim().is_empty() {
                        out.push(Action::TypeAhead(t.clone()));
                    }
                }
            }
        }
    });
    out
}
