#[path = "src/shader_source.rs"]
mod source;

use naga::{ResourceBinding, ShaderStage, back::hlsl};
use std::{
    collections::hash_map::RandomState,
    env,
    ffi::CString,
    fs,
    hash::{BuildHasher, Hasher},
    path::{Path, PathBuf},
    slice,
    time::SystemTime,
};
use windows::{
    Win32::Graphics::Direct3D::{
        Fxc::{D3DCOMPILE_ENABLE_STRICTNESS, D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile},
        ID3DBlob, ID3DInclude,
    },
    core::{PCSTR, s},
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/shader_source.rs");
    for file in source::FILES {
        println!("cargo:rerun-if-changed=src/{file}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    seal(&out);
    let module = naga::front::wgsl::parse_str(source::WGSL)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source::WGSL)));
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), Default::default())
        .validate(&module)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(source::WGSL)));

    // Match the one bind group in Gpu::new and wgpu-hal 29.0.4's DX12 root
    // signature. D3D12 also checks the compiled shaders against that signature.
    let target = |register| hlsl::BindTarget {
        register,
        ..Default::default()
    };
    let mut opts = hlsl::Options {
        fake_missing_bindings: false,
        special_constants_binding: Some(target(1)),
        ..Default::default()
    };
    opts.sampler_heap_target.comparison_samplers = target(2048);
    opts.sampler_buffer_binding_map
        .insert(hlsl::SamplerIndexBufferKey { group: 0 }, target(12));
    for (binding, register) in [0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].into_iter().enumerate() {
        opts.binding_map.insert(
            ResourceBinding {
                group: 0,
                binding: binding as u32,
            },
            hlsl::BindTarget {
                space: if binding == 2 { 255 } else { 0 },
                ..target(register)
            },
        );
    }

    for (stage, entry, profile, file) in [
        (ShaderStage::Vertex, "vertex", s!("vs_5_1"), "vertex.cso"),
        (ShaderStage::Fragment, "glass_capture", s!("ps_5_1"), "glass_capture.cso"),
        (
            ShaderStage::Fragment,
            "shared_home",
            s!("ps_5_1"),
            "home.cso",
        ),
        (
            ShaderStage::Fragment,
            "wallpaper_only",
            s!("ps_5_1"),
            "wallpaper.cso",
        ),
        (
            ShaderStage::Fragment,
            "fragment",
            s!("ps_5_1"),
            "fragment.cso",
        ),
        (
            ShaderStage::Fragment,
            "shared_video_fragment",
            s!("ps_5_1"),
            "video.cso",
        ),
        (
            ShaderStage::Fragment,
            "overlay",
            s!("ps_5_1"),
            "overlay.cso",
        ),
    ] {
        let (module, info) = naga::back::pipeline_constants::process_overrides(
            &module,
            &info,
            Some((stage, entry)),
            &Default::default(),
        )
        .unwrap();
        let pipeline = hlsl::PipelineOptions {
            entry_point: Some((stage, entry.to_owned())),
        };
        let mut hlsl = String::new();
        let reflection = hlsl::Writer::new(&mut hlsl, &opts, &pipeline)
            .write(&module, &info, None)
            .unwrap();
        let name = CString::new(
            reflection
                .entry_point_names
                .into_iter()
                .next()
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let mut code = None;
        let mut errors = None;
        let result = unsafe {
            D3DCompile(
                hlsl.as_ptr().cast(),
                hlsl.len(),
                PCSTR::null(),
                None,
                None::<&ID3DInclude>,
                PCSTR(name.as_ptr().cast()),
                profile,
                D3DCOMPILE_ENABLE_STRICTNESS | D3DCOMPILE_OPTIMIZATION_LEVEL3,
                0,
                &mut code,
                Some(&mut errors),
            )
        };
        let bytes = |blob: &ID3DBlob| unsafe {
            slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
        };
        if let Err(error) = result {
            panic!(
                "FXC {entry}: {error}: {}",
                errors
                    .as_ref()
                    .map(|blob| String::from_utf8_lossy(bytes(blob)))
                    .unwrap_or_default()
            );
        }
        fs::write(out.join(file), bytes(&code.unwrap())).unwrap();
    }
}

/// Seals the catalog credentials into OUT_DIR/sealed.rs, masked with a pad that
/// changes on every build so neither value appears as plain text in the binary.
/// The value comes from IKA_SEED or from ika.build.local (git-ignored).
fn seal(out: &Path) {
    const LOCAL: &str = "ika.build.local";
    println!("cargo:rerun-if-env-changed=IKA_SEED");
    if Path::new(LOCAL).exists() {
        println!("cargo:rerun-if-changed={LOCAL}");
    }
    let seed = env::var("IKA_SEED")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            fs::read_to_string(LOCAL).ok()?.lines().find_map(|line| {
                let (k, v) = line.split_once('=')?;
                (k.trim() == "IKA_SEED").then(|| v.trim().trim_matches(['\'', '"']).to_owned())
            })
        })
        .unwrap_or_default();
    if seed.is_empty() && env::var("PROFILE").as_deref() == Ok("release") {
        println!("cargo:warning=IKA_SEED is not set: this build has no CurseForge access.");
    }
    let mut state = RandomState::new().build_hasher();
    state.write_u128(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    let mut x = state.finish() | 1;
    let mut mask = |name: &str, value: &str| {
        let pad: Vec<u8> = value
            .bytes()
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 32) as u8
            })
            .collect();
        let data: Vec<u8> = value.bytes().zip(&pad).map(|(b, p)| b ^ p).collect();
        format!("static {name}: (&[u8], &[u8]) = (&{data:?}, &{pad:?});\n")
    };
    let code = mask("SEED", &seed) + &mask("BASE", "https://api.curseforge.com/v1");
    fs::write(out.join("sealed.rs"), code).unwrap();
}
