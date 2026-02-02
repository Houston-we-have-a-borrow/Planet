# Houston we have a borrow

This repository contains the implementation of **Planet Core**



## System Overview
The Planet is a `Planet A` type:
- **Energy cell**: multiple
- **Generation**: at most one
- **Rocket**: at most one
- **Combination**: none




## Defense Strategies (Rocket Strategy)

The planet accumulates energy by recharging internal cells during `handle_sunray` events. Depending on the active strategy, incoming energy may be immediately converted into defensive ordinance.

The planet can be configured with specific `RocketStrategy` modes to balance security requirements with explorer support:

| Strategy | Behavior |
| :--- | :--- |
| `Disabled` | No rocket construction.  |
| `Default` | Constructs a rocket only upon asteroid detection. |
| `Safe` | Immediately rebuilds a rocket when energy is available. |
| `EmergencyReserve` | Operates as `Safe`, but maintains 1 energy cell as a hidden reserve. |


```rust
// Example: Configuring a Safe strategy for energy management
let strategy = RocketStrategy::Safe;

let planet = houston_we_have_a_borrow(
        rx_orch, tx_orch, rx_expl,
        id, 
        strategy,
        None
    );
```


## Resource Production (Hydrogen)
The basic resource production is Hydrogen but that can be changed when crating the planet:

To specify the planetary specialization, use the `basic_resource` field. This determines which resource the planet will generate when receiving a `GenerateResourceRequest` from an Explorer.

```rust
//Example: Setting the planet to specialize in Oxygen production
let planet = houston_we_have_a_borrow(
        rx_orch, tx_orch, rx_expl,
        id, 
        strategy,
        Some(BasicResourceType::Oxygen)
    );
```
