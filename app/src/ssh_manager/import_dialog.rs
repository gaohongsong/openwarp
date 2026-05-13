//! ~/.ssh/config 导入对话框 — 显示解析到的 Host 列表,用户勾选后批量创建 Server 节点。

use warp_core::ui::appearance::Appearance;
use warp_core::ui::theme::color::internal_colors;
use warp_ssh_manager::{AuthType, SshConfigHost, SshRepository, SshServerInfo};
use warpui::elements::{
    ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Dismiss, Element, Empty, Fill,
    Flex, Hoverable, MainAxisSize, MouseStateHandle, ParentElement, Radius, Shrinkable, Text,
};
use warpui::fonts::Weight;
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{Entity, SingletonEntity, TypedActionView, View, ViewContext};

use crate::ssh_manager::{SshTreeChangedEvent, SshTreeChangedNotifier};

const DIALOG_WIDTH: f32 = 480.0;
const ROW_PADDING_V: f32 = 6.0;
const ROW_PADDING_H: f32 = 12.0;
const CHECKBOX_SIZE: f32 = 14.0;
const BUTTON_WIDTH: f32 = 80.0;
const BUTTON_HEIGHT: f32 = 28.0;

#[derive(Debug, Clone)]
pub enum SshConfigImportAction {
    ToggleHost(usize),
    SelectAll,
    DeselectAll,
    Import,
    Cancel,
}

#[derive(Debug, Clone)]
pub enum SshConfigImportEvent {
    ImportComplete(Vec<String>),
    Cancelled,
    Error(String),
}

pub struct SshConfigImportDialog {
    hosts: Vec<(SshConfigHost, bool)>,
    row_states: Vec<MouseStateHandle>,
    select_all_btn: MouseStateHandle,
    deselect_all_btn: MouseStateHandle,
    import_btn: MouseStateHandle,
    cancel_btn: MouseStateHandle,
    /// None = 还在加载 / 出错了;Some = 已完成。
    state: ImportState,
}

enum ImportState {
    Ready,
    Empty,
    Error(String),
}

impl SshConfigImportDialog {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let (state, hosts) = match warp_ssh_manager::read_ssh_config_file() {
            Ok(text) => {
                let parsed = warp_ssh_manager::parse_ssh_config(&text);
                if parsed.is_empty() {
                    (ImportState::Empty, Vec::new())
                } else {
                    (ImportState::Ready, parsed)
                }
            }
            Err(e) => (ImportState::Error(e.to_string()), Vec::new()),
        };

        let row_states = (0..hosts.len())
            .map(|_| MouseStateHandle::default())
            .collect();

        // 初始全选
        let hosts: Vec<_> = hosts.into_iter().map(|h| (h, true)).collect();

        Self {
            hosts,
            row_states,
            select_all_btn: MouseStateHandle::default(),
            deselect_all_btn: MouseStateHandle::default(),
            import_btn: MouseStateHandle::default(),
            cancel_btn: MouseStateHandle::default(),
            state,
        }
    }

    fn selected_count(&self) -> usize {
        self.hosts.iter().filter(|(_, s)| *s).count()
    }

    fn on_toggle_host(&mut self, idx: usize, ctx: &mut ViewContext<Self>) {
        if let Some((_, selected)) = self.hosts.get_mut(idx) {
            *selected = !*selected;
            ctx.notify();
        }
    }

    fn on_select_all(&mut self, ctx: &mut ViewContext<Self>) {
        for (_, selected) in &mut self.hosts {
            *selected = true;
        }
        ctx.notify();
    }

    fn on_deselect_all(&mut self, ctx: &mut ViewContext<Self>) {
        for (_, selected) in &mut self.hosts {
            *selected = false;
        }
        ctx.notify();
    }

    fn on_import(&mut self, ctx: &mut ViewContext<Self>) {
        let items: Vec<(String, SshServerInfo)> = self
            .hosts
            .iter()
            .filter(|(_, selected)| *selected)
            .map(|(host, _)| {
                let name = host.alias.clone();
                let info = config_host_to_server_info(host);
                (name, info)
            })
            .collect();

        if items.is_empty() {
            return;
        }

        let result = warp_ssh_manager::with_conn(|c| {
            Ok(SshRepository::create_servers_batch(c, None, &items)?)
        });

        match result {
            Ok(nodes) => {
                let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
                SshTreeChangedNotifier::handle(ctx).update(ctx, |_, ctx| {
                    ctx.emit(SshTreeChangedEvent::TreeChanged);
                });
                ctx.emit(SshConfigImportEvent::ImportComplete(ids));
            }
            Err(e) => {
                log::error!("ssh config import failed: {e:?}");
                ctx.emit(SshConfigImportEvent::Error(e.to_string()));
            }
        }
    }

    fn on_cancel(&mut self, ctx: &mut ViewContext<Self>) {
        ctx.emit(SshConfigImportEvent::Cancelled);
    }

    fn render_host_list(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();

        match &self.state {
            ImportState::Empty => {
                let text = crate::t!("workspace-left-panel-ssh-manager-import-empty");
                return Container::new(
                    Text::new_inline(
                        text,
                        appearance.ui_font_family(),
                        appearance.ui_font_size(),
                    )
                    .with_color(theme.sub_text_color(theme.background()).into())
                    .finish(),
                )
                .with_uniform_padding(16.0)
                .finish();
            }
            ImportState::Error(msg) => {
                return Container::new(
                    Text::new_inline(
                        msg.clone(),
                        appearance.ui_font_family(),
                        appearance.ui_font_size(),
                    )
                    .with_color(theme.ui_error_color().into())
                    .finish(),
                )
                .with_uniform_padding(16.0)
                .finish();
            }
            ImportState::Ready => {}
        }

        let mut col = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_main_axis_size(MainAxisSize::Min);

        for (i, (host, selected)) in self.hosts.iter().enumerate() {
            let row_state = self.row_states.get(i).cloned().unwrap_or_default();
            let is_selected = *selected;

            // 勾选标记
            let check_color = if is_selected {
                theme.accent()
            } else {
                theme.surface_3()
            };
            let checkbox_inner = Container::new(Empty::new().finish())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(3.0)))
                .with_border(
                    warpui::elements::Border::all(1.5).with_border_color(check_color.into()),
                )
                .with_background(if is_selected {
                    theme.accent().into()
                } else {
                    Fill::None
                })
                .finish();
            let checkbox = ConstrainedBox::new(checkbox_inner)
                .with_width(CHECKBOX_SIZE)
                .with_height(CHECKBOX_SIZE)
                .finish();

            let label_text = if host.host_name.as_deref() != Some(&host.alias) {
                format!("{}  ({})", host.alias, host.host_name.as_deref().unwrap_or("?"))
            } else {
                host.alias.clone()
            };
            let label = Text::new_inline(
                label_text,
                appearance.ui_font_family(),
                appearance.ui_font_size(),
            )
            .with_color(theme.main_text_color(theme.background()).into())
            .finish();

            let row = Hoverable::new(row_state, move |_| {
                Container::new(
                    Flex::row()
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_spacing(8.0)
                        .with_child(checkbox)
                        .with_child(label)
                        .with_main_axis_size(MainAxisSize::Min)
                        .finish(),
                )
                .with_padding_top(ROW_PADDING_V)
                .with_padding_bottom(ROW_PADDING_V)
                .with_padding_left(ROW_PADDING_H)
                .finish()
            })
            .with_cursor(warpui::platform::Cursor::PointingHand)
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshConfigImportAction::ToggleHost(i));
            })
            .finish();

            col.add_child(row);
        }

        ConstrainedBox::new(
            Container::new(col.finish())
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.0)))
                .with_background(theme.surface_2())
                .finish(),
        )
        .with_max_height(300.0)
        .finish()
    }
}

impl Entity for SshConfigImportDialog {
    type Event = SshConfigImportEvent;
}

impl TypedActionView for SshConfigImportDialog {
    type Action = SshConfigImportAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            SshConfigImportAction::ToggleHost(i) => self.on_toggle_host(*i, ctx),
            SshConfigImportAction::SelectAll => self.on_select_all(ctx),
            SshConfigImportAction::DeselectAll => self.on_deselect_all(ctx),
            SshConfigImportAction::Import => self.on_import(ctx),
            SshConfigImportAction::Cancel => self.on_cancel(ctx),
        }
    }
}

impl View for SshConfigImportDialog {
    fn ui_name() -> &'static str {
        "SshConfigImportDialog"
    }

    fn on_focus(&mut self, _focus_ctx: &warpui::FocusContext, _ctx: &mut ViewContext<Self>) {}

    fn render(&self, app: &warpui::AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();

        // 顶部按钮行
        let select_all_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Secondary, self.select_all_btn.clone())
            .with_style(UiComponentStyles {
                font_size: Some(12.0),
                height: Some(24.0),
                ..Default::default()
            })
            .with_centered_text_label(crate::t!(
                "workspace-left-panel-ssh-manager-import-select-all"
            ))
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshConfigImportAction::SelectAll);
            })
            .finish();

        let deselect_all_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Secondary, self.deselect_all_btn.clone())
            .with_style(UiComponentStyles {
                font_size: Some(12.0),
                height: Some(24.0),
                ..Default::default()
            })
            .with_centered_text_label(crate::t!(
                "workspace-left-panel-ssh-manager-import-deselect-all"
            ))
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshConfigImportAction::DeselectAll);
            })
            .finish();

        let selection_buttons = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(8.0)
            .with_child(select_all_btn)
            .with_child(deselect_all_btn)
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        let count_label = Text::new_inline(
            format!("{} selected", self.selected_count()),
            appearance.ui_font_family(),
            12.0,
        )
        .with_color(theme.sub_text_color(theme.background()).into())
        .finish();

        let top_row = Flex::row()
            .with_main_axis_size(MainAxisSize::Max)
            .with_main_axis_alignment(warpui::elements::MainAxisAlignment::SpaceBetween)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(selection_buttons)
            .with_child(count_label)
            .finish();

        // 底部操作行
        let import_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Accent, self.import_btn.clone())
            .with_style(UiComponentStyles {
                font_weight: Some(Weight::Bold),
                width: Some(BUTTON_WIDTH),
                height: Some(BUTTON_HEIGHT),
                font_size: Some(13.0),
                font_color: Some(
                    theme
                        .main_text_color(theme.accent())
                        .into_solid(),
                ),
                ..Default::default()
            })
            .with_centered_text_label(crate::t!(
                "workspace-left-panel-ssh-manager-import-button"
            ))
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshConfigImportAction::Import);
            })
            .finish();

        let cancel_btn = appearance
            .ui_builder()
            .button(ButtonVariant::Secondary, self.cancel_btn.clone())
            .with_style(UiComponentStyles {
                font_weight: Some(Weight::Bold),
                width: Some(BUTTON_WIDTH),
                height: Some(BUTTON_HEIGHT),
                font_size: Some(13.0),
                ..Default::default()
            })
            .with_centered_text_label("Cancel".to_string())
            .build()
            .on_click(move |ctx, _, _| {
                ctx.dispatch_typed_action(SshConfigImportAction::Cancel);
            })
            .finish();

        let bottom_row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(8.0)
            .with_child(cancel_btn)
            .with_child(import_btn)
            .with_main_axis_size(MainAxisSize::Min)
            .finish();

        // 整体布局
        let mut content = Flex::column()
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
            .with_child(Container::new(top_row).with_padding_bottom(8.0).finish())
            .with_child(self.render_host_list(appearance));

        content.add_child(
            Container::new(bottom_row)
                .with_padding_top(12.0)
                .finish(),
        );

        let dialog_styles = crate::ui_components::dialog::dialog_styles(appearance);

        let dialog = crate::ui_components::dialog::Dialog::new(
            crate::t!("workspace-left-panel-ssh-manager-import-title").to_string(),
            None,
            dialog_styles,
        )
        .with_child(content.finish())
        .with_width(DIALOG_WIDTH)
        .with_separator()
        .build();

        Dismiss::new(dialog.finish())
            .on_dismiss(|ctx, _| {
                ctx.dispatch_typed_action(SshConfigImportAction::Cancel);
            })
            .finish()
    }
}

/// 把 `SshConfigHost` 转换为 `SshServerInfo`。HostName 缺省用 alias。
fn config_host_to_server_info(host: &SshConfigHost) -> SshServerInfo {
    let has_identity = host.identity_file.is_some();
    SshServerInfo {
        node_id: String::new(),
        host: host
            .host_name
            .clone()
            .unwrap_or_else(|| host.alias.clone()),
        port: host.port.unwrap_or(22),
        username: host.user.clone().unwrap_or_default(),
        auth_type: if has_identity {
            AuthType::Key
        } else {
            AuthType::Password
        },
        key_path: host.identity_file.clone(),
        last_connected_at: None,
        proxy_jump: host.proxy_jump.clone(),
        connect_timeout_secs: host.connect_timeout_secs,
        keepalive_interval_secs: host.server_alive_interval_secs,
        keepalive_count_max: host.server_alive_count_max,
        source: Some("ssh_config".into()),
        host_key_algorithms: host.host_key_algorithms.clone(),
        pubkey_accepted_key_types: host.pubkey_accepted_key_types.clone(),
    }
}
