use std::{borrow::Cow, collections::hash_map::DefaultHasher, hash::{Hash, Hasher}};

use wgpu::BindGroupLayout;

use crate::{state::scene::components::{component::Component, material::{Material, TextureMappingMode, TextureType}}};

use super::{pipeline::Pipeline, wgpu::WGpu};

// derive the builtin feature switches from the material settings (false = every material uses all features)
const AUTO_FEATURES: bool = true;

// default hooks in base.wgsl / shadow.wgsl a material shader can replace
const HOOKS: [&str; 3] = ["custom_vertex", "custom_surface", "custom_fragment"];

// (depth compare, depth write, additive) - same order as RenderPipelineType::Color..ColorAdditiveNoCompare
pub const COLOR_VARIANTS: [(bool, bool, bool); 6] =
[
    (true, true, false),
    (false, true, false),
    (true, false, false),
    (false, false, false),
    (true, false, true),
    (false, false, true),
];

// (depth compare, depth write) - same order as RenderPipelineType::Depth..DepthNoWriteNoCompare
pub const DEPTH_VARIANTS: [(bool, bool); 4] =
[
    (true, true),
    (false, true),
    (true, false),
    (false, false),
];

pub const MAX_CUSTOM_PARAMS: usize = 32;

#[derive(Clone, Copy, PartialEq)]
pub enum ParamType
{
    F32,
    Vec2,
    Vec3,
    Vec4,
    Color,
}

impl ParamType
{
    pub fn size(&self) -> usize
    {
        match self
        {
            ParamType::F32 => 1,
            ParamType::Vec2 => 2,
            ParamType::Vec3 => 3,
            ParamType::Vec4 | ParamType::Color => 4,
        }
    }

    fn wgsl_type(&self) -> &'static str
    {
        match self
        {
            ParamType::F32 => "f32",
            ParamType::Vec2 => "vec2<f32>",
            ParamType::Vec3 => "vec3<f32>",
            ParamType::Vec4 | ParamType::Color => "vec4<f32>",
        }
    }
}

pub struct ShaderParam
{
    pub name: String,
    pub param_type: ParamType,
    pub slot: usize, // first float in custom_params
    pub default: Vec<f32>,
    pub min: f32,
    pub max: f32,
}

// "// @param name: f32|vec2|vec3|vec4|color = default (min..max)" lines of a material shader
pub fn parse_params(source: &str) -> Vec<ShaderParam>
{
    let mut params = vec![];
    let mut slot = 0;

    for line in source.lines()
    {
        let Some(rest) = line.trim().strip_prefix("// @param") else { continue };
        let Some((name, rest)) = rest.split_once(':') else { continue };

        let (rest, range) = match rest.split_once('(')
        {
            Some((rest, range)) => (rest, Some(range.trim().trim_end_matches(')'))),
            None => (rest, None),
        };

        let (type_name, default) = match rest.split_once('=')
        {
            Some((type_name, default)) => (type_name.trim(), Some(default.trim())),
            None => (rest.trim(), None),
        };

        let param_type = match type_name
        {
            "f32" => ParamType::F32,
            "vec2" => ParamType::Vec2,
            "vec3" => ParamType::Vec3,
            "vec4" => ParamType::Vec4,
            "color" => ParamType::Color,
            _ => continue,
        };

        if slot + param_type.size() > MAX_CUSTOM_PARAMS
        {
            break;
        }

        let fill = if param_type == ParamType::Color { 1.0 } else { 0.0 };
        let mut values: Vec<f32> = default.unwrap_or("").split(',').filter_map(|value| value.trim().parse().ok()).collect();
        values.resize(param_type.size(), fill);

        let (min, max) = range.and_then(|range| range.split_once("..")).and_then(|(min, max)| Some((min.trim().parse().ok()?, max.trim().parse().ok()?))).unwrap_or((0.0, 1.0));

        params.push(ShaderParam { name: name.trim().to_string(), param_type, slot, default: values, min, max });
        slot += param_type.size();
    }

    params
}

// "override NAME: bool = true;" lines of a material shader - switches the material can toggle
pub fn parse_features(source: &str) -> Vec<(String, bool)>
{
    source.lines().filter_map(|line|
    {
        let rest = line.trim().strip_prefix("override ")?;
        let (name, rest) = rest.split_once(':')?;
        let (type_name, default) = rest.split_once('=').unwrap_or((rest, "true"));

        if type_name.trim() != "bool"
        {
            return None;
        }

        Some((name.trim().to_string(), !default.contains("false")))
    }).collect()
}

// override values: builtin switches that are off + the switches of the material shader
pub fn constants(material: &Material) -> Vec<(String, f64)>
{
    let data = material.get_data();
    let mut constants = vec![];

    if AUTO_FEATURES
    {
        let builtin =
        [
            ("USE_LIGHTING", !data.unlit_shading),
            ("USE_SHADOWS", data.receive_shadow),
            ("USE_FOG", !data.no_fog),
            ("USE_NORMAL_MAP", material.is_texture_enabled(TextureType::Normal)),
            ("USE_PROJECTED_MAPPING", data.mapping_mode != TextureMappingMode::Uv),
        ];

        constants.extend(builtin.iter().filter(|(_, on)| !on).map(|(name, _)| (name.to_string(), 0.0)));
    }

    constants.extend(feature_constants(material));

    constants
}

// the switches of the material shader only (the shadow pass has no builtin switches)
fn feature_constants(material: &Material) -> Vec<(String, f64)>
{
    let data = material.get_data();

    parse_features(&data.shader_source_code).into_iter().map(|(name, default)|
    {
        let on = data.shader_features.get(&name).copied().unwrap_or(default);
        (name, if on { 1.0 } else { 0.0 })
    }).collect()
}

// 0 = the builtin color pipelines (no material shader, all features on)
pub fn shader_key(material: &Material) -> u64
{
    let source = &material.get_data().shader_source_code;
    let constants = constants(material);

    if source.is_empty() && constants.is_empty()
    {
        return 0;
    }

    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    for (name, value) in &constants
    {
        name.hash(&mut hasher);
        value.to_bits().hash(&mut hasher);
    }

    hasher.finish().max(1)
}

// base.wgsl without the default hooks the material shader replaces + accessors for its params + the material shader
pub fn compose(base: &str, shader: &str) -> String
{
    let replaced: Vec<String> = HOOKS.iter().map(|hook| format!("fn {}(", hook)).filter(|hook| shader.contains(hook.as_str())).collect();

    // filter out custom dummy functions
    let mut source: Vec<&str> = base.lines().filter(|line| !replaced.iter().any(|hook| line.starts_with(hook.as_str()))).collect();
    source.push("\n// ****************************** material shader ******************************\n");

    let accessors: Vec<String> = parse_params(shader).iter().map(param_accessor).collect();

    source.extend(accessors.iter().map(|accessor| accessor.as_str()));
    source.push(shader);

    source.join("\n")
}

// e.g. "fn param_tint() -> vec4<f32> { return vec4<f32>(custom_param(1u), custom_param(2u), custom_param(3u), custom_param(4u)); }"
fn param_accessor(param: &ShaderParam) -> String
{
    let wgsl_type = param.param_type.wgsl_type();

    // one custom_param(slot) per float of the param
    let mut floats = vec![];
    for i in 0..param.param_type.size()
    {
        floats.push(format!("custom_param({}u)", param.slot + i));
    }

    // f32: the float itself, vectors: built from the floats
    let value = if floats.len() == 1 { floats[0].clone() } else { format!("{}({})", wgsl_type, floats.join(", ")) };

    format!("fn param_{}() -> {} {{ return {}; }}", param.name, wgsl_type, value)
}

// "// @param" lines parse_params skips (unknown type, no ':', no slot left) would only show up as a missing param_ function
fn check_params(shader: &str) -> Result<(), String>
{
    let mut params = parse_params(shader).into_iter().peekable();

    let skipped = shader.lines().map(|line| line.trim()).filter(|line| line.starts_with("// @param")).find(|line|
    {
        let name = line["// @param".len()..].split_once(':').map(|(name, _)| name.trim());
        let parsed = params.peek().is_some_and(|param| Some(param.name.as_str()) == name);

        if parsed
        {
            params.next();
        }

        !parsed
    });

    match skipped
    {
        Some(line) => Err(format!("invalid param '{}': type f32|vec2|vec3|vec4|color, at most {} floats in total", line, MAX_CUSTOM_PARAMS)),
        None => Ok(()),
    }
}

pub fn validate(source: &str) -> Result<(), String>
{
    let module = naga::front::wgsl::parse_str(source).map_err(|error| error.emit_to_string(source))?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&module).map_err(|error| error.emit_to_string(source))?;

    Ok(())
}

pub struct CustomPipelines
{
    pub color: Vec<Pipeline>, // COLOR_VARIANTS
    pub depth: Vec<Pipeline>, // DEPTH_VARIANTS - empty without a material shader (builtin depth)
    pub shadow: Option<Pipeline>, // None = builtin shadow
    pub warning: Option<String>, // why the builtin shadow is used
}

// the pipelines of a material - on an error the builtin pipelines are used
pub fn build_pipelines(wgpu: &mut WGpu, base: &String, shadow_base: &String, material: &Material, bind_group_layouts: &[&BindGroupLayout], shadow_bind_group_layouts: &[&BindGroupLayout], max_lights: u32, reverse_z: bool, samples: u32, polygon_mode: wgpu::PolygonMode) -> Result<CustomPipelines, String>
{
    let shader = &material.get_data().shader_source_code;
    let source = Pipeline::prepare_shader(&compose(base, shader), max_lights);

    // only a material shader can break the source
    if !shader.is_empty()
    {
        check_params(shader)?;
        validate(&source)?;
    }

    let name = format!("material shader {}", material.get_base().name);
    let module = create_module(wgpu, &name, &source);

    let constants = constants(material);

    let mut pipelines = CustomPipelines
    {
        color: COLOR_VARIANTS.iter().map(|(depth_compare, depth_write, additive)|
        {
            Pipeline::new_color(wgpu, &name, &module, "fs_main", constants.clone(), bind_group_layouts, *depth_compare, *depth_write, *additive, reverse_z, samples, polygon_mode)
        }).collect(),
        depth: vec![],
        shadow: None,
        warning: None,
    };

    if shader.is_empty()
    {
        return Ok(pipelines);
    }

    // depth pre-pass: same module (vertex hook) with a plain fragment
    pipelines.depth = DEPTH_VARIANTS.iter().map(|(depth_compare, depth_write)|
    {
        Pipeline::new_color(wgpu, &name, &module, "fs_depth", constants.clone(), bind_group_layouts, *depth_compare, *depth_write, false, reverse_z, 1, wgpu::PolygonMode::Fill)
    }).collect();

    // shadow: the material shader has to compile in shadow.wgsl as well (fewer things available there)
    let shadow_source = Pipeline::prepare_shader(&compose(shadow_base, shader), 0);
    if let Err(error) = validate(&shadow_source)
    {
        pipelines.warning = Some(format!("shadow pass (builtin shadow used): {}", error));
        return Ok(pipelines);
    }

    let mut shadow_constants = feature_constants(material);
    shadow_constants.push(("MATERIAL_SHADER".to_string(), 1.0));

    let shadow_module = create_module(wgpu, &format!("{} shadow", name), &shadow_source);
    pipelines.shadow = Some(Pipeline::new_shadow_material(wgpu, &format!("{} shadow", name), &shadow_module, shadow_constants, shadow_bind_group_layouts));

    Ok(pipelines)
}

fn create_module(wgpu: &mut WGpu, name: &str, source: &String) -> wgpu::ShaderModule
{
    wgpu.device().create_shader_module(wgpu::ShaderModuleDescriptor
    {
        label: Some(name),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
    })
}
