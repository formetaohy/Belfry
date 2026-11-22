use belfry_scene::Target;
use belfry_ui::{
    Align, BuildContext, Corners, Element, Icon, Key, Length, Role, State, TextStyle, widget,
};

use crate::message::Message;
use crate::shell::{View, section, text};

pub(crate) fn panel<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let mut children = vec![create(ctx, view)];
    children.push(import(ctx, view));
    children.push(list(ctx, view, "Meshes", assets(view, Asset::Mesh)));
    children.push(list(ctx, view, "Materials", assets(view, Asset::Material)));
    children.push(list(ctx, view, "Images", assets(view, Asset::Image)));
    children.push(list(ctx, view, "Models", assets(view, Asset::Model)));
    children.push(list(ctx, view, "Sounds", assets(view, Asset::Sound)));
    children.push(list(
        ctx,
        view,
        "Environments",
        assets(view, Asset::Environment),
    ));
    children.push(list(ctx, view, "Graphs", assets(view, Asset::Graph)));
    Element::column()
        .width(Length::Fill)
        .gap(8.0)
        .children([section(ctx, "Library", children)])
}

enum Asset {
    Mesh,
    Material,
    Image,
    Model,
    Sound,
    Environment,
    Graph,
}

fn assets(view: &View<'_>, asset: Asset) -> Vec<(String, Target)> {
    let scene = view.scene;
    match asset {
        Asset::Mesh => scene
            .meshes()
            .iter()
            .map(|entry| (entry.id.clone(), Target::Mesh(entry.id.clone())))
            .collect(),
        Asset::Material => scene
            .materials()
            .iter()
            .map(|entry| (entry.id.clone(), Target::Material(entry.id.clone())))
            .collect(),
        Asset::Image => scene
            .images()
            .iter()
            .map(|entry| (entry.id.clone(), Target::Image(entry.id.clone())))
            .collect(),
        Asset::Model => scene
            .models()
            .iter()
            .map(|entry| (entry.id.clone(), Target::Model(entry.id.clone())))
            .collect(),
        Asset::Sound => scene
            .sounds()
            .iter()
            .map(|entry| (entry.id.clone(), Target::Sound(entry.id.clone())))
            .collect(),
        Asset::Environment => scene
            .environments()
            .iter()
            .map(|entry| (entry.id.clone(), Target::Environment(entry.id.clone())))
            .collect(),
        Asset::Graph => scene
            .graphs()
            .iter()
            .map(|entry| (entry.id.clone(), Target::Graph(entry.id.clone())))
            .collect(),
    }
}

fn texture_of(target: &Target) -> Option<String> {
    crate::shell::texture_of(target)
}

fn create<'a>(_ctx: &BuildContext<'_, Message>, _view: &View<'a>) -> Element<'a, Message> {
    let kinds = [
        (Icon::Cube, "mesh", "Mesh"),
        (Icon::Sliders, "material", "Material"),
        (Icon::Folder, "image", "Image"),
        (Icon::Film, "sound", "Sound"),
        (Icon::Globe, "environment", "Environment"),
        (Icon::Chart, "graph", "Graph"),
    ];
    let mut buttons = Vec::new();
    for (icon, kind, tooltip) in kinds {
        buttons.push(
            widget::icon_button(icon, 28.0, Role::Standard, tooltip).press(Message::AddAsset(kind)),
        );
    }
    Element::row()
        .width(Length::Fill)
        .gap(4.0)
        .children(buttons)
}

fn import<'a>(ctx: &BuildContext<'_, Message>, view: &View<'a>) -> Element<'a, Message> {
    let theme = ctx.theme();
    let style = TextStyle::new(theme.font(belfry_ui::Level::Body), theme.palette.on_surface);
    let width = 150.0;
    let import = widget::text_field(view.import, style, width, false)
        .on_commit(|value| Message::Import(String::from(value)));
    let button = widget::button(theme, "Import", Role::Tonal)
        .press(Message::Import(String::from(view.import)));
    let instance_field = widget::text_field(view.prefab, style, width, false)
        .on_commit(|value| Message::Prefab(String::from(value)));
    let instance = widget::button(theme, "Instance", Role::Tonal)
        .press(Message::Prefab(String::from(view.prefab)));
    Element::column().width(Length::Fill).gap(5.0).children([
        Element::row()
            .width(Length::Fill)
            .gap(5.0)
            .align(Align::Center)
            .children([import, button]),
        Element::row()
            .width(Length::Fill)
            .gap(5.0)
            .align(Align::Center)
            .children([instance_field, instance]),
    ])
}

fn list<'a>(
    ctx: &BuildContext<'_, Message>,
    view: &View<'a>,
    title: &str,
    entries: Vec<(String, Target)>,
) -> Element<'a, Message> {
    let theme = ctx.theme();
    let palette = theme.palette;
    if entries.is_empty() {
        return Element::column()
            .width(Length::Fill)
            .gap(2.0)
            .children([text(
                title,
                TextStyle::new(
                    theme.font(belfry_ui::Level::Label),
                    palette.on_surface_variant,
                ),
            )])
            .children([text(
                "none",
                TextStyle::new(theme.font(belfry_ui::Level::Caption), palette.outline),
            )]);
    }
    let mut rows = vec![text(
        title,
        TextStyle::new(
            theme.font(belfry_ui::Level::Label),
            palette.on_surface_variant,
        ),
    )];
    for (id, target) in entries {
        let selected = view.selected == Some(&target);
        let background = if selected {
            theme.surface(Role::PrimaryContainer, State::Selected)
        } else {
            belfry_ui::Surface::NONE
        };
        let mut row = vec![text(
            &id,
            TextStyle::new(
                theme.font(belfry_ui::Level::Caption),
                if selected {
                    palette.on_primary_container
                } else {
                    palette.on_surface
                },
            ),
        )];
        if let Some(texture) = texture_of(&target) {
            row.insert(
                0,
                widget::thumbnail(
                    theme,
                    &texture,
                    Length::Fixed(16.0),
                    Length::Fixed(16.0),
                    Corners::all(theme.metrics.corner_xs),
                ),
            );
        }
        rows.push(
            Element::row()
                .key(Key::new(format!("library.{id}")))
                .width(Length::Fill)
                .height(Length::Fixed(24.0))
                .padding(belfry_ui::Edges::symmetric(6.0, 0.0))
                .gap(6.0)
                .align(Align::Center)
                .background(background)
                .hover_highlight(theme.surface(Role::Standard, State::Hover))
                .children(row)
                .press(Message::Select(target)),
        );
    }
    Element::column()
        .width(Length::Fill)
        .gap(1.0)
        .children(rows)
}
