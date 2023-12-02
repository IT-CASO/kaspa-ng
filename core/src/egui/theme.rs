use kaspa_metrics::MetricGroup;

use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Theme {
    pub visuals: Visuals,

    pub kaspa_color: Color32,
    pub hyperlink_color: Color32,
    pub node_data_color: Color32,
    pub balance_color: Color32,
    pub panel_icon_size: IconSize,
    pub panel_margin_size: f32,
    pub error_icon_size: IconSize,
    pub medium_button_size: Vec2,
    pub large_button_size: Vec2,
    pub panel_footer_height: f32,
    pub panel_editor_size: Vec2,

    pub widget_spacing: f32,
    pub error_color: Color32,
    pub warning_color: Color32,
    pub ack_color: Color32,
    pub nack_color: Color32,

    pub icon_size_large: f32,
    pub icon_color_default: Color32,
    pub status_icon_size: f32,
    pub progress_color: Color32,
    pub graph_frame_color: Color32,
    pub performance_graph_color: Color32,
    pub storage_graph_color: Color32,
    pub connections_graph_color: Color32,
    pub bandwidth_graph_color: Color32,
    pub network_graph_color: Color32,
    pub node_log_font_size: f32,
    pub block_dag_new_block_fill_color: Color32,
    pub block_dag_block_fill_color: Color32,
    pub block_dag_block_stroke_color: Color32,
    pub block_dag_vspc_connect_color: Color32,
    pub block_dag_parent_connect_color: Color32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            visuals: Visuals::light(),
            // visuals: Visuals::dark(),
            kaspa_color: Color32::from_rgb(58, 221, 190),
            // hyperlink_color: Color32::from_rgb(58, 221, 190),
            hyperlink_color: Color32::from_rgb(141, 184, 178),
            // node_data_color : Color32::from_rgb(217, 233,230),
            node_data_color: Color32::WHITE,
            balance_color: Color32::WHITE,
            panel_icon_size: IconSize::new(Vec2::splat(26.)).with_padding(Vec2::new(6., 0.)),
            error_icon_size: IconSize::new(Vec2::splat(64.)).with_padding(Vec2::new(6., 6.)),
            medium_button_size: Vec2::new(100_f32, 30_f32),
            large_button_size: Vec2::new(200_f32, 40_f32),
            panel_footer_height: 72_f32,
            panel_margin_size: 24_f32,
            panel_editor_size: Vec2::new(200_f32, 40_f32),

            widget_spacing: 4_f32,
            error_color: Color32::from_rgb(255, 136, 136),
            warning_color: egui::Color32::from_rgb(255, 255, 136),
            ack_color: Color32::from_rgb(100, 200, 100),
            nack_color: Color32::from_rgb(200, 100, 100),

            icon_size_large: 96_f32,
            icon_color_default: Color32::from_rgb(255, 255, 255),
            status_icon_size: 18_f32,
            progress_color: Color32::from_rgb(21, 82, 71),

            graph_frame_color: Color32::GRAY,
            performance_graph_color: Color32::from_rgb(186, 238, 255),
            storage_graph_color: Color32::from_rgb(255, 231, 186),
            connections_graph_color: Color32::from_rgb(241, 255, 186),
            bandwidth_graph_color: Color32::from_rgb(196, 255, 199),
            network_graph_color: Color32::from_rgb(186, 255, 241),
            node_log_font_size: 15_f32,

            block_dag_new_block_fill_color: Color32::from_rgb(220, 220, 220),
            block_dag_block_fill_color: Color32::from_rgb(0xAD, 0xD8, 0xE6),
            block_dag_block_stroke_color: Color32::from_rgb(15, 84, 77),
            block_dag_vspc_connect_color: Color32::from_rgb(23, 150, 137),
            block_dag_parent_connect_color: Color32::from_rgba_premultiplied(0xAD, 0xD8, 0xE6, 220),
        }
    }
}

impl Theme {
    pub fn panel_icon_size(&self) -> &IconSize {
        &self.panel_icon_size
    }

    pub fn panel_margin_size(&self) -> f32 {
        self.panel_margin_size
    }

    pub fn medium_button_size(&self) -> Vec2 {
        self.medium_button_size
    }

    pub fn large_button_size(&self) -> Vec2 {
        self.large_button_size
    }

    pub fn apply(&self, visuals: &mut Visuals) {
        // let visuals = ui.visuals_mut();
        visuals.hyperlink_color = self.hyperlink_color;
    }
}

static mut THEME: Option<Theme> = None;
#[inline(always)]
pub fn theme() -> &'static Theme {
    unsafe { THEME.get_or_insert_with(Theme::default) }
}

pub fn apply_theme(theme: Theme) {
    unsafe {
        THEME = Some(theme);
    }
}

pub trait MetricGroupExtension {
    fn to_color(&self) -> Color32;
}

impl MetricGroupExtension for MetricGroup {
    fn to_color(&self) -> Color32 {
        match self {
            MetricGroup::System => theme().performance_graph_color,
            MetricGroup::Storage => theme().storage_graph_color,
            MetricGroup::Connections => theme().connections_graph_color,
            MetricGroup::Bandwidth => theme().bandwidth_graph_color,
            MetricGroup::Network => theme().network_graph_color,
        }
    }
}
