use belfry_gpu_device::{
    BufferUsages, GpuBuffer, GpuContext, GpuRequest, Submission, Texture, TextureFormat,
    TextureUsages,
};
use belfry_math::Vec2;
use belfry_ui::{Color, Corners, Element, Key, Length, Level, TextStyle, Ui, canvas};

const SIDE: u32 = 160;
const BACKGROUND: Color = Color::rgb(0.1, 0.1, 0.1);
const SCENE: [u8; 4] = [0, 200, 0, 255];

#[test]
fn a_user_interface_paints_its_surfaces_glyphs_and_scene() {
    let context = GpuContext::open(&GpuRequest::default())
        .unwrap_or_else(|error| panic!("no device paints the interface: {error}"));
    let device = context.device();
    let queue = context.queue();
    let format = TextureFormat::Rgba8Unorm;
    let mut ui: Ui<()> = Ui::new(device, queue, format);
    let target = Texture::new(
        device,
        "belfry ui target",
        SIDE,
        SIDE,
        1,
        format,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let backdrop = Texture::new(
        device,
        "belfry ui backdrop",
        1,
        1,
        1,
        format,
        TextureUsages::SAMPLED | TextureUsages::COPY_DST,
    );
    backdrop.write(queue, &SCENE);

    ui.update(Vec2::splat(SIDE as f32), 1.0, 0.0);
    let element = {
        let ctx = ui.build();
        let theme = *ctx.theme();
        Element::column()
            .key(Key::new("root"))
            .width(Length::Fill)
            .height(Length::Fill)
            .background(belfry_ui::Surface::fill(BACKGROUND))
            .children([
                Element::column()
                    .key(Key::new("glyph"))
                    .width(Length::Fixed(120.0))
                    .height(Length::Fixed(48.0))
                    .children([belfry_ui::text(
                        String::from("H"),
                        TextStyle::new(theme.font(Level::Display), Color::WHITE),
                    )]),
                canvas(Length::Fixed(64.0), 64.0).view_static(|painter, info| {
                    painter.scene(info.rect, Corners::ZERO, [0.0, 0.0, 1.0, 1.0]);
                }),
            ])
    };
    ui.present(element);

    let mut submission = Submission::new(device, "belfry ui paint");
    ui.render(&mut submission, &target, Some(&backdrop));
    let readback = GpuBuffer::new(
        device,
        "belfry ui readback",
        target.readback_bytes(),
        BufferUsages::COPY_DST | BufferUsages::MAP_READ,
    );
    submission.copy_texture(&target, &readback, 0);
    let index = submission.submit(queue);
    let bytes = readback.read(queue, index, target.readback_bytes());
    let pixel = |x: u32, y: u32| {
        let row = (u64::from(y) * target.row_bytes()) as usize;
        let at = row + (x * 4) as usize;
        [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
    };
    let background = pixel(4, SIDE - 4);
    let expected = BACKGROUND.to_array().map(to_byte);
    assert!(
        background[..3]
            .iter()
            .zip(&expected[..3])
            .all(|(held, want)| held.abs_diff(*want) <= 2),
        "the interface fills its surface with {background:?} where the sheet holds {expected:?}",
    );
    let ink = ui
        .bounds("glyph")
        .expect("the interface lays out its glyph");
    let glyph = (0..ink.width() as u32)
        .flat_map(|x| (0..ink.height() as u32).map(move |y| (x, y)))
        .any(|(x, y)| {
            let at = (ink.min.x as u32 + x, ink.min.y as u32 + y);
            pixel(at.0, at.1)[0] > 160 && pixel(at.0, at.1)[1] > 160
        });
    assert!(glyph, "the interface paints the glyphs it shapes");
    let scene = (0..64)
        .flat_map(|x| (64..128).map(move |y| (x, y)))
        .filter(|(x, y)| pixel(*x, *y) == SCENE)
        .count();
    assert!(
        scene > 1000,
        "the interface samples the scene through its texture table, {scene} texels match",
    );
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
