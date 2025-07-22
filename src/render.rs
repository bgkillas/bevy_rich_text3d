use bevy::{
    asset::{AssetId, Assets, RenderAssetUsages},
    color::Srgba,
    ecs::{
        change_detection::DetectChanges,
        system::{Local, Query, Res, ResMut},
        world::{Mut, Ref},
    },
    image::Image,
    math::{FloatOrd, IVec2, Rect, Vec2, Vec3, Vec4},
    render::mesh::{Indices, Mesh, Mesh2d, Mesh3d, PrimitiveTopology, VertexAttributeValues},
};
use cosmic_text::{
    ttf_parser::{Face, GlyphId, OutlineBuilder},
    Attrs, Buffer, Family, FontSystem, LayoutGlyph, Metrics, Shaping, Weight, Wrap,
};
use std::num::NonZero;
use zeno::{Cap, Command as ZCommand, Format, Mask, Stroke, Style, Transform, Vector};

use crate::{
    fetch::FetchedTextSegment,
    mesh_util::ExtractedMesh,
    styling::GlyphEntry,
    text3d::{Text3d, Text3dSegment},
    SegmentStyle, StrokeJoin, Text3dBounds, Text3dDimensionOut, Text3dPlugin, Text3dStyling,
    TextAtlas, TextAtlasHandle, TextRenderer,
};

fn default_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<Vec3>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<Vec3>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<Vec2>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, Vec::<Vec2>::new())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<Vec4>::new())
        .with_inserted_indices(Indices::U16(Vec::new()))
}

fn get_mesh<'t>(
    mesh2d: &mut Option<Mut<Mesh2d>>,
    mesh3d: &mut Option<Mut<Mesh3d>>,
    meshes: &'t mut Assets<Mesh>,
) -> Option<&'t mut Mesh> {
    let mut id = mesh2d
        .as_ref()
        .map(|x| x.id())
        .or_else(|| mesh3d.as_ref().map(|x| x.id()))?;
    if id == AssetId::default() {
        let handle = meshes.add(default_mesh());
        id = handle.id();
        if let Some(handle_2d) = mesh2d {
            handle_2d.0 = handle.clone();
        }
        if let Some(handle_3d) = mesh3d {
            handle_3d.0 = handle;
        }
    }
    meshes.get_mut(id)
}

enum DrawType {
    Fill,
    Stroke(NonZero<u32>),
    Underscore,
    Strikethrough,
}

pub struct DrawRequest {
    request: DrawType,
    color: Srgba,
    offset: Vec2,
    z: f32,
}

const FILLED_RECT: Rect = Rect {
    min: Vec2::ZERO,
    max: Vec2::ZERO,
};

impl Text3dStyling {
    /// Note: Things drawn last gets rendered first.
    pub(crate) fn fill_draw_requests(&self, attrs: &SegmentStyle, requests: &mut Vec<DrawRequest>) {
        requests.clear();
        macro_rules! fill {
            () => {
                if attrs.fill.unwrap_or(self.fill) {
                    if let Some((color, offset)) = self.text_shadow {
                        requests.push(DrawRequest {
                            request: DrawType::Fill,
                            color,
                            offset,
                            z: 0.,
                        });
                    }
                    requests.push(DrawRequest {
                        request: DrawType::Fill,
                        color: attrs.fill_color.unwrap_or(self.color),
                        offset: Vec2::ZERO,
                        z: 0.,
                    });
                }
            };
        }
        macro_rules! stroke {
            () => {
                if let Some(stroke) = attrs.stroke.or(self.stroke) {
                    if let Some((color, offset)) = self.text_shadow {
                        requests.push(DrawRequest {
                            request: DrawType::Stroke(stroke),
                            color,
                            offset,
                            z: 0.,
                        });
                    }
                    requests.push(DrawRequest {
                        request: DrawType::Stroke(stroke),
                        color: attrs.stroke_color.unwrap_or(self.stroke_color),
                        offset: Vec2::ZERO,
                        z: 0.,
                    });
                }
            };
        }
        if self.stroke_offset > 0. {
            fill!();
            stroke!();
        } else {
            stroke!();
            fill!();
        }
        let offset = -self.stroke_offset.abs();
        let mut z = 0.;

        for item in requests.iter_mut().rev() {
            item.z = z;
            z += offset;
        }
    }
}

pub fn text_render(
    settings: Res<Text3dPlugin>,
    font_system: ResMut<TextRenderer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<TextAtlas>>,
    mut text_query: Query<(
        Ref<Text3d>,
        Ref<Text3dBounds>,
        Ref<Text3dStyling>,
        &TextAtlasHandle,
        Option<&mut Mesh2d>,
        Option<&mut Mesh3d>,
        &mut Text3dDimensionOut,
    )>,
    segments: Query<Ref<FetchedTextSegment>>,
    mut draw_requests: Local<Vec<DrawRequest>>,
) {
    let Ok(mut lock) = font_system.0.try_lock() else {
        return;
    };
    let mut redraw = false;
    if font_system.is_changed() {
        redraw = true;
    }
    // Add asynchronously drawn text.
    for (id, atlas, image) in lock.queue.drain(..) {
        let img_id = atlas.image.id();
        images.insert(img_id, image);
        atlases.insert(id, atlas);
        redraw = true;
    }
    let font_system = &mut lock.font_system;
    let scale_factor = settings.scale_factor;
    for (text, bounds, styling, atlas, mut mesh2d, mut mesh3d, mut output) in text_query.iter_mut()
    {
        let Some(atlas) = atlases.get_mut(atlas.0.id()) else {
            return;
        };

        if atlas.image.id() == AssetId::default() || !images.contains(atlas.image.id()) {
            atlas.image = images.add(TextAtlas::empty_image(
                settings.default_atlas_dimension.0,
                settings.default_atlas_dimension.1,
            ))
        };

        let Some(image) = images.get_mut(atlas.image.id()) else {
            return;
        };

        // Change detection.
        if !redraw && !text.is_changed() && !bounds.is_changed() && !styling.is_changed() {
            let mut unchanged = true;
            for segment in &text.segments {
                if let Text3dSegment::Extract(entity) = &segment.0 {
                    if segments.get(*entity).is_ok_and(|x| x.is_changed()) {
                        unchanged = false;
                        break;
                    }
                }
            }
            if unchanged {
                let Some(image) = images.get(atlas.image.id()) else {
                    continue;
                };
                let new_dimension = IVec2::new(image.width() as i32, image.height() as i32);
                if output.atlas_dimension == new_dimension {
                    continue;
                }

                let Some(mesh) = get_mesh(&mut mesh2d, &mut mesh3d, &mut meshes) else {
                    continue;
                };

                let Some(VertexAttributeValues::Float32x2(uv0)) =
                    mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
                else {
                    continue;
                };
                for [x, y] in uv0 {
                    *x *= output.atlas_dimension.x as f32 / new_dimension.x as f32;
                    *y *= output.atlas_dimension.y as f32 / new_dimension.y as f32;
                }
                output.atlas_dimension = new_dimension;
                continue;
            }
        }

        let mut buffer = Buffer::new(
            font_system,
            Metrics::new(styling.size, styling.size * styling.line_height),
        );
        buffer.set_wrap(font_system, Wrap::WordOrGlyph);
        buffer.set_size(font_system, Some(bounds.width), None);
        buffer.set_tab_width(font_system, styling.tab_width);

        buffer.set_rich_text(
            font_system,
            text.segments
                .iter()
                .enumerate()
                .map(|(idx, (text, style))| {
                    (
                        match text {
                            Text3dSegment::String(s) => s.as_str(),
                            Text3dSegment::Extract(e) => segments
                                .get(*e)
                                .map(|x| x.into_inner().as_str())
                                .unwrap_or(""),
                        },
                        style.as_attr(&styling).metadata(idx),
                    )
                }),
            &Attrs::new()
                .family(Family::Name(&styling.font))
                .style(styling.style.into())
                .weight(styling.weight.into()),
            Shaping::Advanced,
            None,
        );

        buffer.shape_until_scroll(font_system, true);

        let Some(mesh) = get_mesh(&mut mesh2d, &mut mesh3d, &mut meshes) else {
            continue;
        };

        let mut mesh = ExtractedMesh::new(mesh);

        let mut width = 0.0f32;
        let mut advance = 0.0f32;
        let mut real_index = 0;

        let mut tess_commands = CommandEncoder::default();
        let mut height = 0.0f32;

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
            for glyph_index in 0..run.glyphs.len() {
                let glyph = &run.glyphs[glyph_index];
                let Some((_, attrs)) = text.segments.get(glyph.metadata) else {
                    continue;
                };
                let dx = -run.line_w * styling.align.as_fac();

                styling.fill_draw_requests(attrs, &mut draw_requests);

                let magic_number = attrs.magic_number.unwrap_or(0.);

                for DrawRequest {
                    request,
                    color,
                    offset,
                    z,
                } in draw_requests.drain(..)
                {
                    let stroke = match request {
                        DrawType::Fill => None,
                        DrawType::Stroke(size) => Some(size),
                        mode @ (DrawType::Strikethrough | DrawType::Underscore) => {
                            let mode = LineMode::from_draw_req(mode);
                            let (min, max) = mode.boundary(run.glyphs, &text.segments, glyph_index);
                            let Some(rect) = mode.get_line_rect(font_system, styling.size, min, max, glyph) else {
                                continue;
                            };
                            mesh.cache_rectangle2(
                                rect,
                                FILLED_RECT,
                                color,
                                z,
                                real_index,
                                advance + min,
                                magic_number,
                                &styling,
                            );
                            continue;
                        },
                    };
                    let Some((pixel_rect, base)) = get_atlas_rect(
                        font_system,
                        scale_factor,
                        &styling,
                        atlas,
                        image,
                        &mut tess_commands,
                        glyph,
                        attrs,
                        stroke,
                    ) else {
                        continue;
                    };

                    let dw = glyph.x + base.x;

                    min_x = min_x.min(dw + dx);
                    max_x = max_x.max(dw + dx + glyph.w);

                    let base =
                        Vec2::new(glyph.x, glyph.y) + base + offset + Vec2::new(dx, -run.line_y);


                    mesh.cache_rectangle(
                        base,
                        pixel_rect,
                        color,
                        scale_factor,
                        z,
                        real_index,
                        advance + dw,
                        magic_number,
                        &styling,
                    );
                }
                real_index += 1;
            }
            advance += run.line_w;
        }

        if max_x < min_x {
            min_x = 0.0;
            max_x = 0.001;
        }

        let dimension = Vec2::new(max_x - min_x, height);
        let center = Vec2::new((max_x + min_x) / 2., -height / 2.);
        let offset = *styling.anchor * dimension - center;
        let bb_min = Vec2::new(min_x, -height);

        mesh.post_process_uv1(&styling, bb_min, dimension);

        if let Some(world_scale) = styling.world_scale {
            mesh.translate(|v| *v = (*v * offset) * world_scale / styling.size);
        } else {
            mesh.translate(|v| *v += offset);
        }

        output.dimension = dimension;
        output.atlas_dimension = IVec2::new(image.width() as i32, image.height() as i32);

        mesh.pixel_to_uv(image);
    }
}

enum LineMode {
    Underscore,
    Strikethrough
}

impl LineMode {
    fn from_draw_req(req: DrawType) -> LineMode {
        match req {
            DrawType::Underscore => LineMode::Underscore,
            DrawType::Strikethrough => LineMode::Strikethrough,
            _ => unreachable!()
        }
    }

    fn validate(&self, style: &SegmentStyle) -> bool {
        match self {
            LineMode::Underscore => style.underscore,
            LineMode::Strikethrough => style.strikethrough,
        }
    }
    
    fn boundary(&self, glyphs: &[LayoutGlyph], segments: &[(Text3dSegment, SegmentStyle)], index: usize) -> (f32, f32) {
        let current = &glyphs[index];
        let mut min = current.x;
        let mut max = current.x + current.w;
        if let Some(prev) = glyphs.get(index.wrapping_sub(1)) {
            if prev.font_id == current.font_id && prev.font_size == current.font_size {
                if let Some((_, style)) = segments.get(prev.metadata) {
                    if self.validate(style) {
                        min = (prev.x + prev.w + current.x) / 2.;
                    }
                }
            } 
        } 
        if let Some(next) = glyphs.get(index.wrapping_add(1)) {
            if next.font_id == current.font_id && next.font_size == current.font_size {
                if let Some((_, style)) = segments.get(next.metadata) {
                    if self.validate(style) {
                        max = (current.x + current.w + next.x) / 2.;
                    }
                }
            } 
        }
        (min, max)
    }

    fn get_line_rect(
        &self, 
        font_system: &mut FontSystem,
        size: f32,
        min: f32,
        max: f32,
        glyph: &LayoutGlyph
    ) -> Option<Rect> {
        font_system
            .db()
            .with_face_data(glyph.font_id, |file, _| {
                let Ok(face) = Face::parse(file, 0) else {
                    return None;
                };
                let metrics = match self {
                    LineMode::Underscore => face.underline_metrics()?,
                    LineMode::Strikethrough => face.strikeout_metrics()?,
                };
                let base = metrics.position as f32 / face.units_per_em() as f32 * size;
                let height = metrics.thickness as f32 / face.units_per_em() as f32 * size;
                Some(Rect { min: Vec2::new(min, base), max: Vec2::new(max, base + height) })
            })
            .flatten()
    }
}

fn get_atlas_rect(
    font_system: &mut FontSystem,
    scale_factor: f32,
    styling: &Text3dStyling,
    atlas: &mut TextAtlas,
    image: &mut Image,
    tess_commands: &mut CommandEncoder,
    glyph: &LayoutGlyph,
    attrs: &SegmentStyle,
    stroke: Option<NonZero<u32>>,
) -> Option<(Rect, Vec2)> {
    atlas
        .glyphs
        .get(&GlyphEntry {
            font: glyph.font_id,
            glyph_id: glyph.glyph_id,
            size: FloatOrd(glyph.font_size),
            weight: styling.weight,
            join: styling.stroke_join,
            stroke,
        })
        .copied()
        .or_else(|| {
            font_system
                .db()
                .with_face_data(glyph.font_id, |file, _| {
                    let Ok(face) = Face::parse(file, 0) else {
                        return None;
                    };
                    cache_glyph(
                        scale_factor,
                        atlas,
                        image,
                        tess_commands,
                        glyph,
                        stroke,
                        styling.stroke_join,
                        attrs.weight.unwrap_or(styling.weight).into(),
                        face,
                    )
                })
                .flatten()
        })
}

pub(crate) fn cache_glyph(
    scale_factor: f32,
    atlas: &mut TextAtlas,
    image: &mut Image,
    tess_commands: &mut CommandEncoder,
    glyph: &cosmic_text::LayoutGlyph,
    stroke: Option<NonZero<u32>>,
    stroke_join: StrokeJoin,
    weight: Weight,
    face: Face,
) -> Option<(Rect, Vec2)> {
    let unit_per_em = face.units_per_em() as f32;
    let entry = GlyphEntry {
        font: glyph.font_id,
        glyph_id: glyph.glyph_id,
        size: FloatOrd(glyph.font_size),
        weight: weight.into(),
        stroke,
        join: stroke_join,
    };
    tess_commands.commands.clear();
    face.outline_glyph(GlyphId(glyph.glyph_id), tess_commands)?;
    let (alpha_map, bb) = if let Some(stroke) = stroke {
        Mask::new(&tess_commands.commands)
            .style(Style::Stroke(Stroke {
                width: stroke.get() as f32 * unit_per_em / 100.,
                start_cap: Cap::Round,
                end_cap: Cap::Round,
                join: stroke_join.into(),
                ..Default::default()
            }))
            .transform(Some(Transform::scale(
                glyph.font_size / unit_per_em * scale_factor,
                glyph.font_size / unit_per_em * scale_factor,
            )))
            .format(Format::Alpha)
            .render()
    } else {
        Mask::new(&tess_commands.commands)
            .transform(Some(Transform::scale(
                glyph.font_size / unit_per_em * scale_factor,
                glyph.font_size / unit_per_em * scale_factor,
            )))
            .format(Format::Alpha)
            .render()
    };
    let (w, h) = (bb.width as usize, bb.height as usize);
    let base = Vec2::new(bb.left as f32, bb.top as f32) / scale_factor;
    let pixel_rect = atlas.cache(image, entry, base, w, h, |buffer, pitch| {
        for x in 0..w {
            for y in 0..h {
                buffer[y * pitch + x * 4 + 3] = alpha_map[y * w + x]
            }
        }
        IVec2::new(w as i32, h as i32)
    });
    Some((pixel_rect, base))
}

#[derive(Debug, Default)]
pub(crate) struct CommandEncoder {
    commands: Vec<ZCommand>,
}

impl OutlineBuilder for CommandEncoder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(ZCommand::MoveTo(Vector::new(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(ZCommand::LineTo(Vector::new(x, y)));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands
            .push(ZCommand::QuadTo(Vector::new(x1, y1), Vector::new(x, y)));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(ZCommand::CurveTo(
            Vector::new(x1, y1),
            Vector::new(x2, y2),
            Vector::new(x, y),
        ));
    }

    fn close(&mut self) {
        self.commands.push(ZCommand::Close);
    }
}
