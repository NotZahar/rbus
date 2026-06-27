pub mod crash_publisher;
pub mod crash_subscriber;
pub mod many_pub_many_sub;
pub mod many_pub_one_sub;
pub mod one_pub_many_sub;
pub mod one_pub_one_sub;
pub mod ping_pong;
pub mod slow_subscriber;

/// Benchmark metadata
pub const SCENARIO_NAMES: &[&str] = &[
    one_pub_one_sub::SCENARIO_NAME,
    one_pub_many_sub::TWO_SUBSCRIBERS_SCENARIO_NAME,
    one_pub_many_sub::FOUR_SUBSCRIBERS_SCENARIO_NAME,
    many_pub_one_sub::TWO_PUBLISHERS_SCENARIO_NAME,
    many_pub_one_sub::FOUR_PUBLISHERS_SCENARIO_NAME,
    many_pub_many_sub::SCENARIO_NAME,
    ping_pong::SCENARIO_NAME,
    slow_subscriber::SCENARIO_NAME,
    crash_subscriber::SCENARIO_NAME,
    crash_publisher::SCENARIO_NAME,
];
