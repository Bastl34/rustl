use std::f32::consts::PI;

use serde::{Deserialize, Serialize};

// rpm of the engine per rad/s at the wheel, before the gear ratio
const RAD_PER_SEC_TO_RPM: f32 = 60.0 / (2.0 * PI);

// how fast the rpm follows its target, 1/s - revving up is quicker than dropping
const RPM_RISE_RATE: f32 = 10.0;
const RPM_FALL_RATE: f32 = 5.0;

// the limiter cuts the drive for this long, which gives the typical bouncing sound
const LIMITER_CUT_TIME: f32 = 0.08;

// how much of the wheel rpm the engine already follows while the clutch still slips
const CLUTCH_FOLLOW: f32 = 0.6;

// below this wheel speed there is nothing to brake with the engine, m/s
const ENGINE_BRAKE_MIN_SPEED: f32 = 1.0;

// in the air an electric motor revs up to this share of its max rpm and holds it - it has no limiter to bounce on
const ELECTRIC_FREE_REV: f32 = 0.85;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum EngineType
{
    #[default]
    Combustion, // torque curve, automatic gearbox, idle
    Electric, // full torque from standstill, one gear, no idle
    Pedal, // a rider - weak, few gears, no idle
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EngineSettings
{
    pub engine_type: EngineType,

    pub idle_rpm: f32,
    pub max_rpm: f32, // rev limiter
    pub max_torque: f32, // Nm at peak_torque_rpm
    pub peak_torque_rpm: f32, // electric: base rpm, the torque falls off above it (constant power)

    pub gear_ratios: Vec<f32>, // forward gears, the first one first - electric uses the first only
    pub reverse_ratio: f32,
    pub final_drive: f32,
    pub efficiency: f32,

    pub shift_up_rpm: f32,
    pub shift_down_rpm: f32,
    pub shift_time: f32, // seconds without drive while shifting

    pub engine_braking: f32, // share of the max torque that drags while off throttle
    pub top_speed: f32, // km/h, 0 = only drag and gearing limit it
    pub max_reverse_speed: f32, // km/h
}

impl Default for EngineSettings
{
    fn default() -> Self
    {
        Self
        {
            engine_type: EngineType::Combustion,

            idle_rpm: 850.0,
            max_rpm: 6500.0,
            max_torque: 250.0,
            peak_torque_rpm: 4000.0,

            gear_ratios: vec![3.5, 2.1, 1.45, 1.1, 0.87],
            reverse_ratio: 3.3,
            final_drive: 3.7,
            efficiency: 0.9,

            shift_up_rpm: 6000.0,
            shift_down_rpm: 2500.0,
            shift_time: 0.25,

            engine_braking: 0.25,
            top_speed: 0.0,
            max_reverse_speed: 25.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EngineState
{
    pub rpm: f32,
    pub gear: i32, // 1.. forward, -1 reverse
    pub throttle: f32, // what actually reaches the wheels, 0 while shifting or limiting
    pub load: f32, // 0..1, smoothed throttle for the sound
    pub shifting: bool,

    shift_timer: f32,
    limiter_timer: f32,
}

impl Default for EngineState
{
    fn default() -> Self
    {
        Self { rpm: 0.0, gear: 1, throttle: 0.0, load: 0.0, shifting: false, shift_timer: 0.0, limiter_timer: 0.0 }
    }
}

impl EngineSettings
{
    fn gear_ratio(&self, gear: i32) -> f32
    {
        if gear < 0
        {
            return self.reverse_ratio;
        }

        let index = (gear.max(1) - 1) as usize;

        match self.engine_type
        {
            EngineType::Electric => self.gear_ratios.first().copied().unwrap_or(1.0),
            _ => self.gear_ratios.get(index).or(self.gear_ratios.last()).copied().unwrap_or(1.0),
        }
    }

    fn forward_gears(&self) -> i32
    {
        match self.engine_type
        {
            EngineType::Electric => 1,
            _ => self.gear_ratios.len().max(1) as i32,
        }
    }

    // Torque at full throttle for an rpm.
    pub fn torque_at(&self, rpm: f32) -> f32
    {
        let rpm = rpm.max(0.0);

        match self.engine_type
        {
            EngineType::Electric | EngineType::Pedal =>
            {
                let base = self.peak_torque_rpm.max(1.0);
                if rpm <= base { self.max_torque } else { self.max_torque * base / rpm }
            }
            EngineType::Combustion =>
            {
                // 60% at idle, full at the peak, 75% at the limiter
                let peak = self.peak_torque_rpm.clamp(self.idle_rpm + 1.0, self.max_rpm - 1.0);

                let factor = if rpm < peak
                {
                    let t = ((rpm - self.idle_rpm) / (peak - self.idle_rpm)).clamp(0.0, 1.0);
                    0.6 + 0.4 * (1.0 - (1.0 - t) * (1.0 - t))
                }
                else
                {
                    let t = ((rpm - peak) / (self.max_rpm - peak)).clamp(0.0, 1.0);
                    1.0 - 0.25 * t * t
                };

                self.max_torque * factor
            }
        }
    }

    fn min_rpm(&self) -> f32
    {
        match self.engine_type
        {
            EngineType::Combustion => self.idle_rpm,
            _ => 0.0,
        }
    }

    // rpm the engine revs to while the clutch still slips, e.g. pulling away
    fn clutch_rpm(&self) -> f32
    {
        (self.idle_rpm + (self.peak_torque_rpm - self.idle_rpm) * 0.25).min(self.max_rpm * 0.5)
    }
}

impl EngineState
{
    pub fn reset(&mut self, settings: &EngineSettings)
    {
        *self = EngineState::default();
        self.rpm = settings.min_rpm();
    }

    // Advances the engine and returns the drive torque summed over the driven wheels, Nm.
    // Positive drives in the direction of the gear, negative is engine braking.
    // speed: along the vehicle forward axis, m/s - wheel_radius: average of the driven wheels - airborne: no driven wheel touches the ground
    pub fn update(&mut self, settings: &EngineSettings, throttle: f32, reverse: bool, speed: f32, wheel_radius: f32, airborne: bool, dt: f32) -> f32
    {
        let throttle = throttle.clamp(0.0, 1.0);
        let wheel_radius = wheel_radius.max(0.05);

        // ********** gear **********
        if reverse && self.gear > 0
        {
            self.gear = -1;
        }
        else if !reverse && self.gear < 0
        {
            self.gear = 1;
        }

        self.shift_timer = (self.shift_timer - dt).max(0.0);
        self.limiter_timer = (self.limiter_timer - dt).max(0.0);

        // rolling against the gear does not turn the engine up - no revving or shifting while rolling back
        let along_gear = (speed * self.gear.signum() as f32).max(0.0);

        let wheel_rpm = along_gear / wheel_radius * RAD_PER_SEC_TO_RPM;
        let coupled_rpm = |gear: i32| wheel_rpm * settings.gear_ratio(gear) * settings.final_drive;

        // in the air the gear stays - nothing to shift for
        if self.gear > 0 && self.shift_timer <= 0.0 && !airborne
        {
            if coupled_rpm(self.gear) > settings.shift_up_rpm && self.gear < settings.forward_gears()
            {
                self.gear += 1;
                self.shift_timer = settings.shift_time;
            }
            else if self.gear > 1 && coupled_rpm(self.gear) < settings.shift_down_rpm && coupled_rpm(self.gear - 1) < settings.shift_up_rpm * 0.9
            {
                self.gear -= 1;
                self.shift_timer = settings.shift_time;
            }
        }

        self.shifting = self.shift_timer > 0.0;

        // ********** rpm **********
        let coupled = coupled_rpm(self.gear);

        let mut target = coupled.max(settings.min_rpm());

        // a slipping clutch lets the engine rev above the wheels while pulling away - rising with the wheels,
        // a fixed slip rpm would hold the sound flat until the wheels catch up with it
        if settings.engine_type == EngineType::Combustion && self.gear.abs() == 1
        {
            target = target.max(settings.idle_rpm + throttle * (settings.clutch_rpm() - settings.idle_rpm) + coupled * CLUTCH_FOLLOW);
        }

        // free revving while shifting - the throttle keeps pushing a little
        if self.shifting
        {
            target = coupled.max(settings.min_rpm()) + throttle * 300.0;
        }

        // nothing holds the engine back in the air: the throttle revs it toward the limiter, off the throttle it falls back to the wheels
        if airborne
        {
            let top = if settings.engine_type == EngineType::Combustion { settings.max_rpm } else { settings.max_rpm * ELECTRIC_FREE_REV };
            target = target.max(coupled + throttle * (top - coupled).max(0.0));
        }

        let rate = if target > self.rpm { RPM_RISE_RATE } else { RPM_FALL_RATE };
        self.rpm += (target - self.rpm) * (1.0 - (-rate * dt).exp());
        self.rpm = self.rpm.clamp(0.0, settings.max_rpm * 1.02);

        if self.rpm >= settings.max_rpm && self.limiter_timer <= 0.0
        {
            self.limiter_timer = LIMITER_CUT_TIME;
        }

        // ********** limits **********
        let speed_kmh = along_gear * 3.6;
        let over_top_speed = settings.top_speed > 0.0 && self.gear > 0 && speed_kmh > settings.top_speed;
        let over_reverse_speed = self.gear < 0 && speed_kmh > settings.max_reverse_speed;

        let cut = self.shifting || self.limiter_timer > 0.0 || over_top_speed || over_reverse_speed;

        self.throttle = if cut { 0.0 } else { throttle };
        self.load += (throttle - self.load) * (1.0 - (-8.0 * dt).exp());

        // ********** torque **********
        let ratio = settings.gear_ratio(self.gear) * settings.final_drive * settings.efficiency;

        if self.throttle > 0.0
        {
            return settings.torque_at(self.rpm) * self.throttle * ratio;
        }

        // only while moving with the gear - rolling back against it, the drag would push the vehicle further
        if throttle <= 0.0 && along_gear > ENGINE_BRAKE_MIN_SPEED && !self.shifting
        {
            let rev = ((self.rpm - settings.min_rpm()) / (settings.max_rpm - settings.min_rpm()).max(1.0)).clamp(0.0, 1.0);
            return -settings.engine_braking * settings.max_torque * ratio * rev;
        }

        0.0
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    fn run(settings: &EngineSettings, state: &mut EngineState, speed: f32, seconds: f32) -> f32
    {
        let mut torque = 0.0;
        let steps = (seconds * 60.0) as usize;

        for _ in 0..steps
        {
            torque = state.update(settings, 1.0, false, speed, 0.33, false, 1.0 / 60.0);
        }

        torque
    }

    #[test]
    fn the_gearbox_shifts_up_with_speed()
    {
        let settings = EngineSettings::default();
        let mut state = EngineState::default();
        state.reset(&settings);

        run(&settings, &mut state, 2.0, 1.0);
        assert_eq!(state.gear, 1);

        run(&settings, &mut state, 40.0, 3.0);
        assert!(state.gear >= 4, "gear {}", state.gear);
    }

    #[test]
    fn the_torque_peaks_at_the_peak_rpm()
    {
        let settings = EngineSettings::default();

        assert!(settings.torque_at(settings.peak_torque_rpm) > settings.torque_at(settings.idle_rpm));
        assert!(settings.torque_at(settings.peak_torque_rpm) > settings.torque_at(settings.max_rpm));
    }

    // accelerating from standstill, the rpm has to climb with the speed - no flat stretch at the clutch rpm
    #[test]
    fn pulling_away_revs_up_with_the_speed()
    {
        let settings = EngineSettings::default();
        let mut state = EngineState::default();
        state.reset(&settings);

        let mut rpms = vec![];
        for step in 0..120
        {
            let speed = step as f32 / 60.0 * 5.0; // 0 to 10 m/s in 2 s
            state.update(&settings, 1.0, false, speed, 0.33, false, 1.0 / 60.0);

            if step % 15 == 0 && step >= 30
            {
                rpms.push(state.rpm);
            }
        }

        assert_eq!(state.gear, 1);
        assert!(rpms.windows(2).all(|pair| pair[1] > pair[0] + 50.0), "rpm stalls while pulling away: {:?}", rpms);
    }

    #[test]
    fn reverse_is_limited()
    {
        let settings = EngineSettings::default();
        let mut state = EngineState::default();
        state.reset(&settings);

        let torque = state.update(&settings, 1.0, true, -20.0, 0.33, false, 1.0 / 60.0);
        assert_eq!(torque, 0.0);
        assert_eq!(state.gear, -1);
    }

    // a failed climb: rolling back in first gear must neither be pushed further nor shift up
    #[test]
    fn rolling_back_against_the_gear_gets_no_engine_drag()
    {
        let settings = EngineSettings::default();
        let mut state = EngineState::default();
        state.reset(&settings);

        let mut torque = 0.0;
        for _ in 0..120
        {
            torque = state.update(&settings, 0.0, false, -12.0, 0.33, false, 1.0 / 60.0);
        }

        assert_eq!(torque, 0.0);
        assert_eq!(state.gear, 1);
        assert!(state.rpm < settings.idle_rpm * 1.1, "rpm {}", state.rpm);
    }

    // a jump at 20 m/s: full throttle revs up to the limiter in the gear it took off in, letting go drops back to the wheels
    #[test]
    fn in_the_air_the_throttle_revs_freely_in_the_gear()
    {
        for engine_type in [EngineType::Combustion, EngineType::Electric]
        {
            let settings = EngineSettings { engine_type, ..EngineSettings::default() };
            let mut state = EngineState::default();
            state.reset(&settings);

            run(&settings, &mut state, 20.0, 3.0);
            let (gear, ground_rpm) = (state.gear, state.rpm);

            let mut highest: f32 = 0.0;
            for _ in 0..60
            {
                state.update(&settings, 1.0, false, 20.0, 0.33, true, 1.0 / 60.0);
                highest = highest.max(state.rpm);
            }
            assert_eq!(state.gear, gear, "{:?} shifted in the air", engine_type);

            let top = if engine_type == EngineType::Combustion { settings.max_rpm } else { settings.max_rpm * ELECTRIC_FREE_REV };
            assert!(highest > top * 0.97 && highest <= settings.max_rpm * 1.02, "{:?}: {:.0} rpm on the ground, {:.0} in the air", engine_type, ground_rpm, highest);

            for _ in 0..60
            {
                state.update(&settings, 0.0, false, 20.0, 0.33, true, 1.0 / 60.0);
            }
            assert!((state.rpm - ground_rpm).abs() < ground_rpm * 0.1 + 50.0, "{:?}: off the throttle {:.0} rpm, on the ground it was {:.0}", engine_type, state.rpm, ground_rpm);
        }
    }
}
