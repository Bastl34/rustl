use crate::state::scene::scene_controller::vehicle_controller::{SteeringAxis, VehicleBalanceSettings, VehicleChassisSettings, VehicleController, VehicleSteeringSettings, VehicleDriftSettings, VehicleDrive, VehicleType};

use super::engine::{EngineSettings, EngineType};

// Starting values per vehicle type - everything stays editable afterwards.
impl VehicleController
{
    pub fn apply_preset(&mut self)
    {
        let vehicle_type = self.vehicle_type;

        // ********** common defaults **********
        self.drive = VehicleDrive::Rear;
        self.engine = EngineSettings::default();
        self.balance = VehicleBalanceSettings { enabled: vehicle_type.is_two_wheeler(), ..VehicleBalanceSettings::default() };
        self.steering.axis = if vehicle_type.is_two_wheeler() { SteeringAxis::Handlebar } else { SteeringAxis::Column };
        self.steering.ratio = if vehicle_type.is_two_wheeler() { 1.0 } else { 12.0 };
        self.steering.high_speed_factor = 0.35;
        self.steering.speed = VehicleSteeringSettings::default().speed;
        self.drift = VehicleDriftSettings::default();
        self.chassis.angular_damping = VehicleChassisSettings::default().angular_damping;

        match vehicle_type
        {
            VehicleType::Car =>
            {
                self.chassis.mass = 1300.0;
                self.suspension.stiffness = 35.0; self.suspension.compression = 2.2; self.suspension.relaxation = 3.2;
                self.suspension.rest_length = 0.3; self.suspension.travel = 0.2;
                self.tires.grip = 2.0; self.tires.side_grip = 1.0;
                self.steering.max_angle = 35.0;
                self.brakes.brake_force = 13000.0; self.brakes.handbrake_force = 12000.0; self.brakes.air_drag = 0.9; self.brakes.rolling_resistance = 200.0;
            }
            VehicleType::SportsCar =>
            {
                self.chassis.mass = 1400.0;
                self.suspension.stiffness = 55.0; self.suspension.compression = 3.0; self.suspension.relaxation = 4.2;
                self.suspension.rest_length = 0.22; self.suspension.travel = 0.12;
                self.tires.grip = 2.4; self.tires.side_grip = 1.1;
                self.steering.max_angle = 32.0;
                self.brakes.brake_force = 20000.0; self.brakes.handbrake_force = 13000.0; self.brakes.air_drag = 0.7; self.brakes.rolling_resistance = 200.0;

                self.engine.idle_rpm = 800.0; self.engine.max_rpm = 7500.0;
                self.engine.max_torque = 520.0; self.engine.peak_torque_rpm = 5000.0;
                self.engine.gear_ratios = vec![3.2, 2.2, 1.6, 1.25, 1.0, 0.82];
                self.engine.final_drive = 3.4;
                self.engine.shift_up_rpm = 7000.0; self.engine.shift_down_rpm = 4000.0; self.engine.shift_time = 0.15;
            }
            VehicleType::ElectricCar =>
            {
                self.chassis.mass = 1800.0;
                self.suspension.stiffness = 40.0; self.suspension.compression = 2.5; self.suspension.relaxation = 3.5;
                self.suspension.rest_length = 0.28; self.suspension.travel = 0.18;
                self.tires.grip = 2.1; self.tires.side_grip = 1.0;
                self.steering.max_angle = 35.0;
                self.brakes.brake_force = 18000.0; self.brakes.handbrake_force = 16000.0; self.brakes.air_drag = 0.75; self.brakes.rolling_resistance = 220.0;

                self.drive = VehicleDrive::All;
                self.engine.engine_type = EngineType::Electric;
                self.engine.idle_rpm = 0.0; self.engine.max_rpm = 15000.0;
                self.engine.max_torque = 420.0; self.engine.peak_torque_rpm = 5000.0;
                self.engine.gear_ratios = vec![9.0];
                self.engine.final_drive = 1.0;
                self.engine.engine_braking = 0.35; // regeneration
            }
            VehicleType::Bus | VehicleType::Truck =>
            {
                let bus = vehicle_type == VehicleType::Bus;

                self.chassis.mass = if bus { 12000.0 } else { 9000.0 };
                self.suspension.stiffness = 25.0; self.suspension.compression = 2.5; self.suspension.relaxation = 3.5;
                self.suspension.rest_length = 0.35; self.suspension.travel = 0.25;
                self.tires.grip = 1.5; self.tires.side_grip = 1.0;
                self.steering.max_angle = 40.0; self.steering.high_speed_factor = 0.5;
                self.brakes.brake_force = if bus { 90000.0 } else { 70000.0 }; self.brakes.handbrake_force = 40000.0; self.brakes.air_drag = 3.5; self.brakes.rolling_resistance = 1500.0;
                self.drift.counter_steer = 0.0;

                self.diesel(if bus { 1600.0 } else { 1400.0 });
                self.engine.top_speed = if bus { 90.0 } else { 100.0 };
            }
            VehicleType::MultiAxle =>
            {
                self.chassis.mass = 16000.0;
                self.suspension.stiffness = 22.0; self.suspension.compression = 2.0; self.suspension.relaxation = 3.0;
                self.suspension.rest_length = 0.45; self.suspension.travel = 0.35;
                self.tires.grip = 1.7; self.tires.side_grip = 1.0;
                self.steering.max_angle = 30.0; self.steering.high_speed_factor = 0.5;
                self.brakes.brake_force = 120000.0; self.brakes.handbrake_force = 60000.0; self.brakes.air_drag = 4.0; self.brakes.rolling_resistance = 2500.0;
                self.drift.counter_steer = 0.0;

                self.drive = VehicleDrive::All;
                self.diesel(2500.0);
                self.engine.top_speed = 100.0;
            }
            VehicleType::Motorcycle | VehicleType::Trike =>
            {
                let trike = vehicle_type == VehicleType::Trike;

                self.chassis.mass = if trike { 380.0 } else { 250.0 };

                // two wheels carry half the weight each - stiffer, so the sag stays well inside the travel
                if trike { self.suspension.stiffness = 55.0; self.suspension.compression = 3.2; self.suspension.relaxation = 4.3; }
                else { self.suspension.stiffness = 80.0; self.suspension.compression = 4.5; self.suspension.relaxation = 6.0; }
                self.suspension.rest_length = 0.25; self.suspension.travel = 0.15;
                self.tires.grip = 2.0; self.tires.side_grip = 1.2;
                self.steering.max_angle = 30.0; self.steering.high_speed_factor = 0.2;
                if !trike { self.balance.max_lean = 40.0; self.balance.roll_rate = 90.0; self.steering.speed = 2.0; }
                self.brakes.brake_force = 2800.0; self.brakes.handbrake_force = 900.0; self.brakes.air_drag = 0.35; self.brakes.rolling_resistance = 40.0;
                self.drift.counter_steer = 0.0;

                self.engine.idle_rpm = 1200.0; self.engine.max_rpm = 11000.0;
                self.engine.max_torque = 90.0; self.engine.peak_torque_rpm = 8000.0;
                self.engine.gear_ratios = vec![2.8, 2.0, 1.6, 1.35, 1.18, 1.05];
                self.engine.final_drive = 4.5;
                self.engine.shift_up_rpm = 10000.0; self.engine.shift_down_rpm = 5000.0; self.engine.shift_time = 0.12;
            }
            VehicleType::Scooter =>
            {
                self.chassis.mass = 190.0;
                self.suspension.stiffness = 120.0; self.suspension.compression = 5.7; self.suspension.relaxation = 7.8;
                self.suspension.rest_length = 0.18; self.suspension.travel = 0.1;
                self.tires.grip = 1.8; self.tires.side_grip = 1.1;
                self.steering.max_angle = 35.0; self.steering.high_speed_factor = 0.3;
                self.balance.max_lean = 35.0; self.balance.roll_rate = 75.0; self.steering.speed = 2.0;
                self.brakes.brake_force = 1800.0; self.brakes.handbrake_force = 600.0; self.brakes.air_drag = 0.35; self.brakes.rolling_resistance = 30.0;
                self.drift.counter_steer = 0.0;

                // a variator keeps the rpm up - three steps come close enough
                self.engine.idle_rpm = 1800.0; self.engine.max_rpm = 9000.0;
                self.engine.max_torque = 25.0; self.engine.peak_torque_rpm = 6500.0;
                self.engine.gear_ratios = vec![14.0, 10.0, 7.5];
                self.engine.final_drive = 1.0;
                self.engine.shift_up_rpm = 8000.0; self.engine.shift_down_rpm = 5500.0; self.engine.shift_time = 0.05;
                self.engine.top_speed = 70.0;
            }
            VehicleType::Bicycle =>
            {
                self.chassis.mass = 90.0;
                self.suspension.stiffness = 160.0; self.suspension.compression = 6.6; self.suspension.relaxation = 9.0;
                self.suspension.rest_length = 0.1; self.suspension.travel = 0.08;
                self.tires.grip = 1.6; self.tires.side_grip = 1.1;
                self.steering.max_angle = 40.0; self.steering.high_speed_factor = 0.4;
                self.balance.max_lean = 30.0; self.balance.roll_rate = 60.0; self.steering.speed = 2.0;
                self.brakes.brake_force = 1000.0; self.brakes.handbrake_force = 400.0; self.brakes.air_drag = 0.25; self.brakes.rolling_resistance = 10.0;
                self.drift.counter_steer = 0.0;

                // the rpm is the pedal cadence
                self.engine.engine_type = EngineType::Pedal;
                self.engine.idle_rpm = 0.0; self.engine.max_rpm = 130.0;
                self.engine.max_torque = 65.0; self.engine.peak_torque_rpm = 70.0; // about 480 W, a rider pushing hard
                self.engine.gear_ratios = vec![0.9, 0.65, 0.45];
                self.engine.final_drive = 1.0;
                self.engine.shift_up_rpm = 100.0; self.engine.shift_down_rpm = 50.0; self.engine.shift_time = 0.1;
                self.engine.engine_braking = 0.0;
                self.engine.top_speed = 40.0;
                self.engine.max_reverse_speed = 0.0;
            }
            VehicleType::Kart =>
            {
                self.chassis.mass = 180.0;
                self.chassis.angular_damping = 1.0; // a handbrake slide stays a drift instead of a spin
                self.suspension.stiffness = 70.0; self.suspension.compression = 3.5; self.suspension.relaxation = 4.5;
                self.suspension.rest_length = 0.12; self.suspension.travel = 0.06;
                self.tires.grip = 2.4; self.tires.side_grip = 1.2;
                self.drift.handbrake_side_grip = 0.6; self.drift.counter_steer = 1.0; self.drift.grip_recovery = 3.0; self.drift.throttle_hold = 0.6;
                self.steering.max_angle = 30.0; self.steering.high_speed_factor = 0.5; self.steering.speed = 5.0;
                self.brakes.brake_force = 2200.0; self.brakes.handbrake_force = 1600.0; self.brakes.air_drag = 0.4; self.brakes.rolling_resistance = 30.0;

                self.engine.idle_rpm = 1800.0; self.engine.max_rpm = 12000.0;
                self.engine.max_torque = 22.0; self.engine.peak_torque_rpm = 9000.0;
                self.engine.gear_ratios = vec![2.2, 1.6, 1.25, 1.05];
                self.engine.final_drive = 8.0;
                self.engine.shift_up_rpm = 11000.0; self.engine.shift_down_rpm = 7000.0; self.engine.shift_time = 0.1;
                self.engine.top_speed = 95.0;
                self.engine.max_reverse_speed = 15.0;
            }
            VehicleType::Tank =>
            {
                self.chassis.mass = 45000.0;
                self.suspension.stiffness = 30.0; self.suspension.compression = 3.0; self.suspension.relaxation = 4.0;
                self.suspension.rest_length = 0.45; self.suspension.travel = 0.3;
                self.tires.grip = 1.8; self.tires.side_grip = 1.2;
                self.steering.max_angle = 0.0;
                self.brakes.brake_force = 400000.0; self.brakes.handbrake_force = 300000.0; self.brakes.air_drag = 5.0; self.brakes.rolling_resistance = 15000.0;
                self.drift.counter_steer = 0.0;

                self.drive = VehicleDrive::Tracked;
                self.diesel(4500.0);
                self.engine.gear_ratios = vec![5.0, 3.0, 1.8, 1.2];
                self.engine.final_drive = 6.0;
                self.engine.top_speed = 60.0;
            }
        }

        self.assign_wheel_roles();
        self.mark_physics_dirty();
    }

    fn diesel(&mut self, torque: f32)
    {
        self.engine.idle_rpm = 600.0;
        self.engine.max_rpm = 2700.0;
        self.engine.max_torque = torque;
        self.engine.peak_torque_rpm = 1300.0;
        self.engine.gear_ratios = vec![4.5, 2.6, 1.6, 1.0, 0.75];
        self.engine.final_drive = 5.2;
        self.engine.shift_up_rpm = 2300.0;
        self.engine.shift_down_rpm = 1100.0;
        self.engine.shift_time = 0.4;
        self.engine.engine_braking = 0.3;
        self.engine.max_reverse_speed = 15.0;
    }
}
