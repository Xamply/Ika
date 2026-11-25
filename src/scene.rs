use serde_json::Value;
use std::{collections::HashMap, fs, io, path::Path, time::Duration};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let (part, rest) = self
            .0
            .split_at_checked(count)
            .ok_or_else(|| invalid("Archivo truncado"))?;
        self.0 = rest;
        Ok(part)
    }

    fn uint(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn float(&mut self) -> io::Result<f32> {
        Ok(f32::from_bits(self.uint()?))
    }

    fn magic(&mut self, expected: &[u8]) -> io::Result<()> {
        if self.take(expected.len())? != expected {
            return Err(invalid("Formato TEX no compatible con la prueba"));
        }
        Ok(())
    }
}

fn entries(pkg: &[u8]) -> io::Result<HashMap<&str, &[u8]>> {
    let mut reader = Reader(pkg);
    let length = reader.uint()? as usize;
    let magic = reader.take(length)?;
    if magic.len() != 8 || !magic.starts_with(b"PKGV") {
        return Err(invalid(
            "Selecciona un paquete de escena PKGV de Wallpaper Engine",
        ));
    }
    let count = reader.uint()?;
    if count > 4096 {
        return Err(invalid("El paquete contiene demasiadas entradas"));
    }
    let mut index = Vec::new();
    for _ in 0..count {
        let length = reader.uint()? as usize;
        let name = std::str::from_utf8(reader.take(length)?).map_err(io::Error::other)?;
        let offset = reader.uint()? as usize;
        let len = reader.uint()? as usize;
        index.push((name, offset, len));
    }
    index
        .into_iter()
        .map(|(name, offset, len)| {
            let data = reader
                .0
                .get(offset..)
                .and_then(|bytes| bytes.get(..len))
                .ok_or_else(|| invalid(format!("Entrada fuera del paquete: {name}")))?;
            Ok((name, data))
        })
        .collect()
}

fn json(entries: &HashMap<&str, &[u8]>, name: &str) -> io::Result<Value> {
    let bytes = entries
        .get(name)
        .ok_or_else(|| invalid(format!("Falta {name} dentro del paquete")))?;
    serde_json::from_slice(bytes).map_err(io::Error::other)
}

pub struct Frame {
    pub rect: [f32; 4],
    end: f64,
}

pub struct Scene {
    pub title: String,
    pub size: [u32; 2],
    pub texture: Vec<u8>,
    pub mip_count: u32,
    pub frame_aspect: f32,
    pub frames: Vec<Frame>,
    pub period: f64,
    pub nearest: bool,
    pub motion: Option<Chisa>,
}

pub struct Chisa {
    pub mask_size: [u32; 2],
    pub masks: Vec<u8>,
    shake: [[f32; 5]; 4],
    foliage: [f32; 4],
    iris: [f32; 4],
    shimmer: [[f32; 4]; 2],
    shine: [f32; 4],
    ember_origin: [f32; 2],
    dust_origin: [f32; 2],
}

impl Scene {
    pub fn shared(size: [u32; 2]) -> Self {
        let [width, height] = size.map(|n| n as f32);
        Self {
            title: "Wallpaper Engine oficial | Chisa".into(),
            size,
            texture: Vec::new(),
            mip_count: 1,
            frame_aspect: width / height,
            frames: vec![Frame {
                rect: [0.0, 0.0, width, height],
                end: 1.0,
            }],
            period: 1.0,
            nearest: false,
            motion: None,
        }
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        if fs::metadata(path)?.len() > 64 * 1024 * 1024 {
            return Err(invalid(
                "El paquete supera los 64 MiB admitidos en esta prueba",
            ));
        }
        let data = fs::read(path)?;
        let entries = entries(&data)?;
        let project: Value = match fs::read(path.with_file_name("project.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Value::Null,
            Err(e) => return Err(e),
        };
        let scene_file =
            project["file"]
                .as_str()
                .unwrap_or(if entries.contains_key("scene.json") {
                    "scene.json"
                } else {
                    "gifscene.json"
                });
        let scene = json(&entries, scene_file)?;
        if project["workshopid"] == "3608457742"
            || entries.contains_key("materials/G53ctgoaEAA__jt.tex")
        {
            return Self::chisa(&entries, &scene);
        }
        let objects = scene["objects"]
            .as_array()
            .filter(|objects| objects.len() == 1)
            .ok_or_else(|| invalid("La prueba admite solo una capa de sprites animada"))?;
        let image = objects[0]["image"]
            .as_str()
            .filter(|_| objects[0]["effects"].is_null())
            .ok_or_else(|| invalid("Capas con efectos o partículas no compatibles"))?;
        let model = json(&entries, image)?;
        let material = json(
            &entries,
            model["material"]
                .as_str()
                .ok_or_else(|| invalid("Falta el material"))?,
        )?;
        let pass = &material["passes"][0];
        let texture = pass["textures"][0]
            .as_str()
            .filter(|_| pass["shader"] == "genericimage" && pass["combos"]["spritesheet"] == 1)
            .ok_or_else(|| invalid("El material debe ser un spritesheet animado, no un video"))?;
        let texture = format!("materials/{texture}.tex");
        let atlas = entries
            .get(texture.as_str())
            .ok_or_else(|| invalid("Falta el atlas original"))?;
        let title = project["title"]
            .as_str()
            .unwrap_or("Wallpaper Engine")
            .to_owned();
        Self::atlas(title, atlas)
    }

    fn atlas(title: String, data: &[u8]) -> io::Result<Self> {
        let mut reader = Reader(data);
        reader.magic(b"TEXV0005\0TEXI0001\0")?;
        let format = reader.uint()?;
        let flags = reader.uint()?;
        let size = [reader.uint()?, reader.uint()?];
        let image_size = [reader.uint()?, reader.uint()?];
        reader.take(4)?;
        reader.magic(b"TEXB0004\0")?;
        if format != 0
            || flags & 4 == 0
            || flags & 32 != 0
            || size != image_size
            || size.iter().any(|&n| n == 0 || n > 8192)
            || reader.uint()? != 1
            || reader.uint()? != u32::MAX
            || reader.uint()? != 0
            || reader.uint()? != 1
        {
            return Err(invalid(
                "Se espera un único atlas RGBA, sin video ni mipmaps",
            ));
        }
        if [reader.uint()?, reader.uint()?] != size {
            return Err(invalid("Dimensiones del atlas incoherentes"));
        }
        let compressed = reader.uint()?;
        let declared = reader.uint()? as usize;
        let length = reader.uint()? as usize;
        let expected = size[0] as usize * size[1] as usize * 4;
        if expected > 64 * 1024 * 1024 {
            return Err(invalid("Atlas demasiado grande"));
        }
        let raw = reader.take(length)?;
        let rgba = match compressed {
            0 if length == expected => raw.to_vec(),
            1 if declared == expected => {
                lz4_flex::block::decompress(raw, expected).map_err(io::Error::other)?
            }
            _ => return Err(invalid("Compresión o tamaño de mipmap no compatible")),
        };
        if rgba.len() != expected {
            return Err(invalid("El atlas decodificado no tiene el tamaño esperado"));
        }
        reader.magic(b"TEXS0003\0")?;
        let frame_count = reader.uint()?;
        let frame_size = [reader.uint()?, reader.uint()?];
        if !(2..=1024).contains(&frame_count) || frame_size.contains(&0) {
            return Err(invalid("Tabla de animación inválida"));
        }
        let mut frames = Vec::new();
        let mut period = 0.0;
        let w = frame_size[0] as f32;
        let h = frame_size[1] as f32;
        for _ in 0..frame_count {
            let sheet = reader.uint()?;
            let duration = reader.float()?;
            let x = reader.float()?;
            let y = reader.float()?;
            let x_axis = [reader.float()?, reader.float()?];
            let y_axis = [reader.float()?, reader.float()?];
            if sheet != 0
                || !duration.is_finite()
                || !(0.01..=60.0).contains(&duration)
                || !x.is_finite()
                || !y.is_finite()
                || x < 0.0
                || y < 0.0
                || x_axis != [w, 0.0]
                || y_axis != [0.0, h]
                || x + w > size[0] as f32
                || y + h > size[1] as f32
            {
                return Err(invalid("Cuadro fuera del atlas o transformado"));
            }
            period += duration as f64;
            frames.push(Frame {
                rect: [x, y, w, h],
                end: period,
            });
        }
        Ok(Self {
            title,
            size,
            texture: rgba,
            mip_count: 1,
            frame_aspect: w / h,
            frames,
            period,
            nearest: flags & 1 != 0,
            motion: None,
        })
    }

    pub fn frame_at(&self, elapsed: Duration) -> (usize, Duration) {
        if self.motion.is_some() {
            let tick = elapsed.as_secs_f64() * 60.0;
            return (
                tick.floor() as usize,
                Duration::from_secs_f64((1.0 - tick.fract()) / 60.0),
            );
        }
        let phase = elapsed.as_secs_f64() % self.period;
        let index = self.frames.partition_point(|frame| frame.end <= phase);
        let delay = (self.frames[index].end - phase).max(0.001);
        (index, Duration::from_secs_f64(delay))
    }

    fn chisa(entries: &HashMap<&str, &[u8]>, scene: &Value) -> io::Result<Self> {
        let layer = &scene["objects"][0];
        let image = layer["image"]
            .as_str()
            .ok_or_else(|| invalid("Falta la capa de Chisa"))?;
        let model = json(entries, image)?;
        let material = json(
            entries,
            model["material"]
                .as_str()
                .ok_or_else(|| invalid("Falta el material de Chisa"))?,
        )?;
        let texture = material["passes"][0]["textures"][0]
            .as_str()
            .ok_or_else(|| invalid("Falta la textura de Chisa"))?;
        let (size, mip_count, texture) = read_texture(entries, texture, 4)?;
        let effects = layer["effects"]
            .as_array()
            .ok_or_else(|| invalid("Faltan los efectos de Chisa"))?;
        let pick = |kind: &str, n: usize| {
            let path = format!("effects/{kind}/effect.json");
            effects
                .iter()
                .filter(|effect| effect["file"] == path && effect["visible"] != false)
                .filter_map(|effect| effect["passes"].as_array())
                .nth(n)
                .and_then(|items| items.first())
                .ok_or_else(|| invalid(format!("Falta el efecto {kind} {}", n + 1)))
        };
        let mut masks = Vec::new();
        let mut mask_size = [0, 0];
        let mut append_mask = |pass: &Value, slot: usize, flow: bool| -> io::Result<()> {
            let name = pass["textures"][slot]
                .as_str()
                .ok_or_else(|| invalid("Falta una máscara de Chisa"))?;
            let (dimensions, mips, texels) = read_texture(entries, name, if flow { 8 } else { 9 })?;
            if mips != 1 {
                return Err(invalid("La máscara debe tener una sola resolución"));
            }
            if mask_size == [0, 0] {
                mask_size = dimensions;
            }
            if dimensions != mask_size {
                return Err(invalid("Las máscaras no comparten dimensiones"));
            }
            if flow {
                masks.extend(texels);
            } else {
                for value in texels {
                    masks.extend([value, 0]);
                }
            }
            Ok(())
        };
        let mut shake = [[0.0; 5]; 4];
        for (i, wave) in shake.iter_mut().enumerate() {
            let pass = pick("shake", i)?;
            append_mask(pass, 1, true)?;
            let c = &pass["constantshadervalues"];
            let bounds = numbers::<2>(&c["bounds"])?;
            let friction = numbers::<2>(&c["friction"])?;
            *wave = [
                number(c, "speed")?,
                number(c, "strength")?,
                friction[0],
                bounds[0],
                bounds[1],
            ];
        }
        let foliage = pick("foliagesway", 0)?;
        let iris = pick("iris", 0)?;
        let first_shimmer = pick("shimmer", 0)?;
        let second_shimmer = pick("shimmer", 1)?;
        let shine = pick("shine", 0)?;
        for (pass, slot) in [
            (foliage, 1),
            (iris, 1),
            (first_shimmer, 1),
            (second_shimmer, 1),
            (shine, 1),
        ] {
            append_mask(pass, slot, false)?;
        }
        let shimmer = |c: &Value| -> io::Result<[f32; 4]> {
            let c = &c["constantshadervalues"];
            Ok([
                number(c, "ui_editor_properties_direction")?,
                number(c, "ui_editor_properties_speed")?,
                number(c, "ui_editor_properties_delay")?,
                number(c, "ui_editor_properties_brightness")?,
            ])
        };
        let foliage = &foliage["constantshadervalues"];
        let iris = &iris["constantshadervalues"];
        let scale = numbers::<2>(&iris["scale"])?;
        let shine = &shine["constantshadervalues"];
        let origin = |i: usize| -> io::Result<[f32; 2]> {
            let [x, y, _] = numbers::<3>(&scene["objects"][i]["origin"])?;
            Ok([x / size[0] as f32, 1.0 - y / size[1] as f32])
        };
        Ok(Self {
            title: "Wuthering Waves | Chisa".to_owned(),
            size,
            texture,
            mip_count,
            frame_aspect: size[0] as f32 / size[1] as f32,
            frames: vec![Frame {
                rect: [0.0, 0.0, size[0] as f32, size[1] as f32],
                end: 0.0,
            }],
            period: 0.0,
            nearest: false,
            motion: Some(Chisa {
                mask_size,
                masks,
                shake,
                foliage: [
                    number(foliage, "strength")?,
                    number(foliage, "ratio")?,
                    number(foliage, "speeduv")?,
                    number(foliage, "phase")?,
                ],
                iris: [
                    number(iris, "speed")?,
                    number(iris, "rough")?,
                    number(iris, "noiseamount")?,
                    scale[0],
                ],
                shimmer: [shimmer(first_shimmer)?, shimmer(second_shimmer)?],
                shine: [
                    number(shine, "noisespeed")?,
                    number(shine, "raythreshold")?,
                    number(shine, "noiseamount")?,
                    number(shine, "noisescale")?,
                ],
                ember_origin: origin(1)?,
                dust_origin: origin(2)?,
            }),
        })
    }
}

impl Chisa {
    pub fn parameters(&self, t: f32) -> [[f32; 4]; 8] {
        let amplitudes = self.shake.map(|[speed, strength, friction, lo, hi]| {
            let angle = t * speed;
            let curve = angle.sin() * 0.498 + 0.5;
            let offset = if angle.cos() >= 0.0 {
                curve.powf(friction)
            } else {
                1.0 - (1.0 - curve).powf(friction)
            };
            let factor = ((offset - lo) / (hi - lo)).clamp(0.0, 1.0) * 2.0 - 1.0;
            factor * strength * strength
        });
        let [speed, rough, noise, scale] = self.iris;
        let time = t * speed;
        let k = time.floor();
        let blend = ((0.5 - (time.fract() * std::f32::consts::PI).cos() * 0.5 - (1.0 - rough))
            / rough)
            .clamp(0.0, 1.0);
        let blend = blend * blend * (3.0 - 2.0 * blend);
        let eyes = [1.0_f32, 2.0].map(|phase| {
            let start = (1.9 * k).sin() + (2.5 * k + phase).sin();
            let end = (1.9 * (k + 1.0)).sin() + (2.5 * (k + 1.0) + phase).sin();
            start + (end - start) * blend
        });
        [
            amplitudes,
            self.foliage,
            [
                (eyes[0] + time.sin() * noise) * scale * 0.001,
                (eyes[1] + time.cos() * noise) * scale * 0.001,
                t,
                1.0,
            ],
            self.shimmer[0],
            self.shimmer[1],
            self.shine,
            [
                self.ember_origin[0],
                self.ember_origin[1],
                self.dust_origin[0],
                self.dust_origin[1],
            ],
            [0.0; 4],
        ]
    }
}

fn number(value: &Value, key: &str) -> io::Result<f32> {
    value[key]
        .as_f64()
        .map(|n| n as f32)
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid(format!("Parámetro inválido: {key}")))
}

fn numbers<const N: usize>(value: &Value) -> io::Result<[f32; N]> {
    let bad = || invalid("Vector de efecto inválido");
    let mut parts = value.as_str().ok_or_else(bad)?.split_whitespace();
    let mut result = [0.0; N];
    for number in &mut result {
        *number = parts
            .next()
            .and_then(|part| part.parse::<f32>().ok())
            .filter(|n| n.is_finite())
            .ok_or_else(bad)?;
    }
    if parts.next().is_some() {
        return Err(bad());
    }
    Ok(result)
}

fn read_texture(
    entries: &HashMap<&str, &[u8]>,
    name: &str,
    format: u32,
) -> io::Result<([u32; 2], u32, Vec<u8>)> {
    let path = format!("materials/{name}.tex");
    let mut reader = Reader(
        entries
            .get(path.as_str())
            .ok_or_else(|| invalid(format!("Falta {path}")))?,
    );
    reader.magic(b"TEXV0005\0TEXI0001\0")?;
    if reader.uint()? != format || reader.uint()? & 36 != 0 {
        return Err(invalid(format!("Formato inesperado en {path}")));
    }
    let size = [reader.uint()?, reader.uint()?];
    if size.iter().any(|&n| n == 0 || n > 8192) {
        return Err(invalid("Textura demasiado grande"));
    }
    reader.take(12)?;
    reader.magic(b"TEXB0004\0")?;
    if reader.uint()? != 1 || reader.uint()? != u32::MAX || reader.uint()? != 0 {
        return Err(invalid(format!(
            "Encabezado de textura inesperado en {path}"
        )));
    }
    let mips = reader.uint()?;
    if !(1..=12).contains(&mips) {
        return Err(invalid("Cantidad de mipmaps inválida"));
    }
    let mut data = Vec::new();
    for level in 0..mips {
        let [w, h] = size.map(|n| (n >> level).max(1));
        if [reader.uint()?, reader.uint()?] != [w, h] {
            return Err(invalid(format!("Dimensiones inesperadas en {path}")));
        }
        let compression = reader.uint()?;
        let declared = reader.uint()? as usize;
        let len = reader.uint()? as usize;
        let payload = reader.take(len)?;
        let expected = match format {
            4 => w.div_ceil(4) * h.div_ceil(4) * 16,
            8 => w * h * 2,
            9 => w * h,
            _ => return Err(invalid("Formato de canales no compatible")),
        } as usize;
        let start = data.len();
        if data.len() + expected > 64 * 1024 * 1024 {
            return Err(invalid("Textura demasiado grande"));
        }
        match compression {
            0 if len == expected => data.extend_from_slice(payload),
            1 if declared == expected => {
                data.resize(start + expected, 0);
                let written = lz4_flex::block::decompress_into(payload, &mut data[start..])
                    .map_err(io::Error::other)?;
                if written != expected {
                    return Err(invalid(format!("Tamaño de mipmap inesperado en {path}")));
                }
            }
            _ => return Err(invalid(format!("Compresión inesperada en {path}"))),
        }
    }
    Ok((size, mips, data))
}
