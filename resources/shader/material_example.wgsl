// example material shader: world space stripes + rim light
// hooks (all optional, each one replaces a default):
//   custom_vertex(vertex: ptr<function, CustomVertex>)                 world space position / normal / uv - color, depth and shadow pass
//   custom_surface(in: CustomSurfaceInput, surface: ptr<function, CustomSurface>) base color (+alpha) / ambient / normal before the lighting - discard allowed
//   custom_fragment(in: CustomSurfaceInput, color: vec3<f32>) -> vec3<f32> final color before fog and tone mapping
// CustomVertex: position, normal (world), local_position, uv, uv1, color (vertex), object_position, model (object -> world), instance_color (per instance data, alpha = visibility)
// CustomSurfaceInput: position, local_position, normal, uv, color, view_dir, object_position
// available: param_<name>(), get_time(), instance_seed(object_position) (0..1 per object), is_shadow_pass(), material, tex_custom0..3 (+ _sampler), PI
// "discard" makes the material alpha tested, a "// @transparent" line makes it transparent (blend mode Blend)

// @param stripe_color: color = 1,0.4,0.1,1
// @param stripe_scale: f32 = 4.0 (0.1..50)
// @param rim_strength: f32 = 0.5 (0..5)

// switches show up as checkboxes, switched off parts are compiled out
override RIM_LIGHT: bool = true;

fn custom_surface(in: CustomSurfaceInput, surface: ptr<function, CustomSurface>)
{
    let stripe = step(0.5, fract(in.position.y * param_stripe_scale()));
    (*surface).base_color = mix((*surface).base_color, param_stripe_color(), stripe);
}

fn custom_fragment(in: CustomSurfaceInput, color: vec3<f32>) -> vec3<f32>
{
    if (!RIM_LIGHT)
    {
        return color;
    }

    let rim = 1.0 - max(dot(in.normal, in.view_dir), 0.0);
    return color + param_stripe_color().rgb * pow(rim, 3.0) * param_rim_strength();
}
