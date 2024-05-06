use std::f32::consts::PI;

use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, FromRepr};

use super::math::bezier_interpolate;

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr)]
pub enum Easing
{
    None,
    InSine,
    OutSine,
    InOutSine,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InExpo,
    OutExpo,
    InOutExpo,
    InCirc,
    OutCirc,
    InOutCirc,
    InBack,
    OutBack,
    InOutBack,
    InElastic,
    OutElastic,
    InOutElastic,
    InBounce,
    OutBounce,
    InOutBounce,

    InOutFast
}

pub fn easing(easing_type: Easing, x: f32) -> f32
{
    match easing_type
    {
        Easing::None => x,
        Easing::InSine => ease_in_sine(x),
        Easing::OutSine => ease_out_sine(x),
        Easing::InOutSine => ease_in_out_sine(x),
        Easing::InQuad => ease_in_quad(x),
        Easing::OutQuad => ease_out_quad(x),
        Easing::InOutQuad => ease_in_out_quad(x),
        Easing::InCubic => ease_in_cubic(x),
        Easing::OutCubic => ease_out_cubic(x),
        Easing::InOutCubic => ease_in_out_cubic(x),
        Easing::InQuart => ease_in_quart(x),
        Easing::OutQuart => ease_out_quart(x),
        Easing::InOutQuart => ease_in_out_quart(x),
        Easing::InQuint => ease_in_quint(x),
        Easing::OutQuint => ease_out_quint(x),
        Easing::InOutQuint => ease_in_out_quint(x),
        Easing::InExpo => ease_in_expo(x),
        Easing::OutExpo => ease_out_expo(x),
        Easing::InOutExpo => ease_in_out_expo(x),
        Easing::InCirc => ease_in_circ(x),
        Easing::OutCirc => ease_out_circ(x),
        Easing::InOutCirc => ease_in_out_circ(x),
        Easing::InBack => ease_in_back(x),
        Easing::OutBack => ease_out_back(x),
        Easing::InOutBack => ease_in_out_back(x),
        Easing::InElastic => ease_in_elastic(x),
        Easing::OutElastic => ease_out_elastic(x),
        Easing::InOutElastic => ease_in_out_elastic(x),
        Easing::InBounce => ease_in_bounce(x),
        Easing::OutBounce => ease_out_bounce(x),
        Easing::InOutBounce => ease_in_out_bounce(x),
        Easing::InOutFast => ease_in_out_fast(x),
    }
}

pub fn get_easing_as_string_vec() -> Vec<String>
{
    let vec: Vec<Easing> = Easing::iter().collect::<Vec<_>>();
    vec.iter().map(|easing| { easing.to_string() }).collect::<Vec<_>>()
}

// based on: https://easings.net/

// https://easings.net/#easeInSine
pub fn ease_in_sine(x: f32) -> f32
{
    1.0 - ((x * PI) / 2.0).cos()
}

// https://easings.net/#easeOutSine
pub fn ease_out_sine(x: f32) -> f32
{
    ((x * PI) / 2.0).sin()
}

// https://easings.net/#easeInOutSine
pub fn ease_in_out_sine(x: f32) -> f32
{
    -((PI * x).cos() - 1.0) / 2.0
}

// https://easings.net/#easeInQuad
pub fn ease_in_quad(x: f32) -> f32
{
    x * x
}

// https://easings.net/#easeOutQuad
pub fn ease_out_quad(x: f32) -> f32
{
    1.0 - (1.0 - x) * (1.0 - x)
}

// https://easings.net/#easeInOutQuad
pub fn ease_in_out_quad(x: f32) -> f32
{
    if x < 0.5
    {
        2.0 * x * x
    }
    else
    {
        1.0 - (-2.0 * x + 2.0).powf(2.0) / 2.0
    }
}

// https://easings.net/#easeInCubic
pub fn ease_in_cubic(x: f32) -> f32
{
    x * x * x
}

// https://easings.net/#easeOutCubic
pub fn ease_out_cubic(x: f32) -> f32
{
    1.0 - (1.0 - x).powi(3)
}

// https://easings.net/#easeInOutCubic
pub fn ease_in_out_cubic(x: f32) -> f32
{
    if x < 0.5
    {
        4.0 * x * x * x
    }
    else
    {
        1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
    }
}

// https://easings.net/#easeInQuart
pub fn ease_in_quart(x: f32) -> f32
{
    x * x * x * x
}

// https://easings.net/#easeOutQuart
pub fn ease_out_quart(x: f32) -> f32
{
    1.0 - (1.0 - x).powi(4)
}

// https://easings.net/#easeInOutQuart
pub fn ease_in_out_quart(x: f32) -> f32
{
    if x < 0.5
    {
        8.0 * x * x * x * x
    }
    else
    {
        1.0 - (-2.0 * x + 2.0).powi(4) / 2.0
    }
}

// https://easings.net/#easeInQuint
pub fn ease_in_quint(x: f32) -> f32
{
    x * x * x * x * x
}

// https://easings.net/#easeOutQuint
pub fn ease_out_quint(x: f32) -> f32
{
    1.0 - (1.0 - x).powi(5)
}

// https://easings.net/#easeInOutQuint
pub fn ease_in_out_quint(x: f32) -> f32
{
    if x < 0.5
    {
        16.0 * x * x * x * x * x
    }
    else
    {
        1.0 - (-2.0 * x + 2.0).powi(5) / 2.0
    }
}

// https://easings.net/#easeInExpo
pub fn ease_in_expo(x: f32) -> f32
{
    if x == 0.0
    {
        0.0
    }
    else
    {
        (2.0_f32).powf(10.0 * x - 10.0)
    }
}

// https://easings.net/#easeOutExpo
pub fn ease_out_expo(x: f32) -> f32
{
    if x == 1.0
    {
        1.0
    }
    else
    {
        1.0 - (2.0_f32).powf(-10.0 * x)
    }
}

// https://easings.net/#easeInOutExpo
pub fn ease_in_out_expo(x: f32) -> f32
{
    if x == 0.0
    {
        0.0
    }
    else
    {
        if x == 1.0
        {
            1.0
        }
        else
        {
            if x < 0.5
            {
                (2.0_f32).powf(20.0 * x - 10.0) / 2.0
            }
            else
            {
                (2.0 - (2.0_f32).powf(-20.0 * x + 10.0)) / 2.0
            }
        }
    }
}

// https://easings.net/#easeInCirc
pub fn ease_in_circ(x: f32) -> f32
{
    1.0 - (1.0 - (x).powi(2)).sqrt()
}

// https://easings.net/#easeOutCirc
pub fn ease_out_circ(x: f32) -> f32
{
    (1.0 - (x - 1.0).powi(2)).sqrt()
}

// https://easings.net/#easeInOutCirc
pub fn ease_in_out_circ(x: f32) -> f32
{
    if x < 0.5
    {
        (1.0 - (1.0 - (2.0 * x).powi(2)).sqrt()) / 2.0
    }
    else
    {
        ((1.0 - (-2.0 * x + 2.0).powi(2)).sqrt() + 1.0) / 2.0
    }
}

// https://easings.net/#easeInBack
pub fn ease_in_back(x: f32) -> f32
{
    let c1: f32 = 1.70158;
    let c3 = c1 + 1.0;

    c3 * x * x * x - c1 * x * x
}

// https://easings.net/#easeOutBack
pub fn ease_out_back(x: f32) -> f32
{
    let c1: f32 = 1.70158;
    let c3 = c1 + 1.0;

    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}

// https://easings.net/#easeInOutBack
pub fn ease_in_out_back(x: f32) -> f32
{
    let c1: f32 = 1.70158;
    let c2 = c1 * 1.525;

    if x < 0.5
    {
        ((2.0 * x).powi(2) * ((c2 + 1.0) * 2.0 * x - c2)) / 2.0
    }
    else
    {
        ((2.0 * x - 2.0).powi(2) * ((c2 + 1.0) * (x * 2.0 - 2.0) + c2) + 2.0) / 2.0
    }
}

// https://easings.net/#easeInElastic
pub fn ease_in_elastic(x: f32) -> f32
{
    let c4 = (2.0 * PI) / 3.0;

    if x == 0.0
    {
        0.0
    }
    else
    {
        if x == 1.0
        {
            1.0
        }
        else
        {
            -(2.0_f32).powf(10.0 * x - 10.0) * ((x * 10.0 - 10.75) * c4).sin()
        }
    }
}

// https://easings.net/#easeOutElastic
pub fn ease_out_elastic(x: f32) -> f32
{
    let c4 = (2.0 * PI) / 3.0;

    if x == 0.0
    {
        0.0
    }
    else
    {
        if x == 1.0
        {
            1.0
        }
        else
        {
            (2.0_f32).powf(-10.0 * x) * ((x * 10.0 - 0.75) * c4).sin() + 1.0
        }
    }
}

// https://easings.net/#easeInOutElastic
pub fn ease_in_out_elastic(x: f32) -> f32
{
    let c5: f32 = (2.0 * PI) / 4.5;

    if x == 0.0
    {
        0.0
    }
    else
    {
        if x == 1.0
        {
            1.0
        }
        else
        {
            if x < 0.5
            {
                -((2.0_f32).powf(20.0 * x - 10.0) * ((20.0 * x - 11.125) * c5).sin()) / 2.0
            }
            else
            {
                ((2.0_f32).powf(-20.0 * x + 10.0) * ((20.0 * x - 11.125) * c5).sin()) / 2.0 + 1.0
            }
        }
    }
}

// https://easings.net/#easeOutBounce
pub fn ease_out_bounce(x: f32) -> f32
{
    let n1: f32 = 7.5625;
    let d1: f32 = 2.75;

    if x < 1.0 / d1
    {
        n1 * x * x
    }
    else if x < 2.0 / d1
    {
        let x = x - 1.5 / d1;
        n1 * x * x + 0.75
    }
    else if x < 2.5 / d1
    {
        let x = x - 2.25 / d1;
        n1 * x * x + 0.9375
    }
    else
    {
        let x = x - 2.625 / d1;
        n1 * x * x + 0.984375
    }
}

// https://easings.net/#easeInBounce
pub fn ease_in_bounce(x: f32) -> f32
{
    1.0 - ease_out_bounce(1.0 - x)
}

// https://easings.net/#easeInOutBounce
pub fn ease_in_out_bounce(x: f32) -> f32
{
    if x < 0.5
    {
        (1.0 - ease_out_bounce(1.0 - 2.0 * x)) / 2.0
    }
    else
    {
        (1.0 + ease_out_bounce(2.0 * x - 1.0)) / 2.0
    }
}

// https://cubic-bezier.com/#0,.7,1,.3
pub fn ease_in_out_fast(x: f32) -> f32
{
    bezier_interpolate(x, 0.0, 0.7, 1.0, 0.3)
}