use common_game::components::planet::*;
use common_game::components::resource::{BasicResource, BasicResourceType, Combinator, Generator};
use common_game::components::rocket::Rocket;
use common_game::logging::{ActorType, Channel, EventType, LogEvent, Payload};
use common_game::protocols::messages::{
    ExplorerToPlanet, OrchestratorToPlanet, PlanetToExplorer, PlanetToOrchestrator,
};
use crossbeam_channel::{Receiver, Sender};
use std::fmt::{Display, Formatter};

/// Controls how the planet AI manages rocket construction.
///
/// - `Disabled`: never build rockets.
/// - `Default`: build a rocket only when an asteroid is coming.
/// - `Safe`: always rebuild a rocket when there isn't any.
/// - `EmergencyReserve`: same as `Safe`, but keeps one extra full cell reserved.
#[derive(Debug, PartialEq, Eq ,Default, Clone)]
pub enum RocketStrategy {
    /// Do not generate rockets under any condition.
    Disabled,

    /// Normal behavior: generate a rocket only when an asteroid is coming.
    #[default]
    Default,

    /// Always rebuild a rocket when there isn't any
    Safe,

    /// Same as `Safe`, but preserves one fully charged cell for emergencies.
    EmergencyReserve,
}

struct PlanetCoreThinkingModel {
    basic_resource: BasicResourceType,
    rocket_strategy: RocketStrategy,
    running: bool,
}

impl Display for RocketStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}


impl PlanetCoreThinkingModel {
    fn charged_count( &mut self,
            state: &mut PlanetState,) -> u32 {
        let mut count = 0;
       state.cells_iter().for_each(|x| {
           if x.is_charged() {
               count += 1;
           }
       });
        count
    }
}
impl PlanetAI for PlanetCoreThinkingModel {
    fn handle_orchestrator_msg(
        &mut self,
        state: &mut PlanetState,
        _generator: &Generator,
        _combinator: &Combinator,
        msg: OrchestratorToPlanet,
    ) -> Option<PlanetToOrchestrator> {
        match msg {
            OrchestratorToPlanet::Sunray(sunray) => {
                let mut p = Payload::new();
                p.insert("type".to_string(), "SunrayAck".to_string());
                p.insert(
                    "rocketStrategy".to_string(),
                    self.rocket_strategy.to_string(),
                );
                p.insert(
                    "energyCellCountBeforeAck".to_string(),
                    format!("{}", self.charged_count(state)),
                );
                p.insert(
                    "rocketBeforeAck".to_string(),
                    format!("{}", state.has_rocket()),
                );
                let mut log = LogEvent::new(
                    ActorType::Planet,
                    state.id(),
                    ActorType::Orchestrator,
                    0u32.to_string(),
                    EventType::MessagePlanetToOrchestrator,
                    Channel::Debug,
                    Payload::new(), //fake payload
                );

                // Try to charge an empty cell
                let leftover = state.charge_cell(sunray);

                // Helper: check if this strategy allows building
                let can_build = |strategy: &RocketStrategy| -> bool {
                    match strategy {
                        RocketStrategy::Disabled => false,
                        RocketStrategy::Default => false, // never build on Sunray
                        RocketStrategy::Safe => true,
                        RocketStrategy::EmergencyReserve => true,
                    }
                };

                // CASE A — leftover == None  → at least one cell was uncharged
                if leftover.is_none() {
                    // Should we try building a rocket now?
                    if state.can_have_rocket()
                        && !state.has_rocket()
                        && can_build(&self.rocket_strategy)
                    {
                        let _ = try_build_rocket(state);
                    }
                } else {
                    // CASE B — leftover == Some(sunray) → all cells were full
                    if state.can_have_rocket()
                        && !state.has_rocket()
                        && can_build(&self.rocket_strategy)
                    {
                        if let Some(cell_index) = try_build_rocket(state) {
                            // Recharge the cell used to build the rocket with the leftover sunray
                            state.cell_mut(cell_index).charge(leftover.unwrap());
                        }
                    }
                }

                p.insert(
                    "energyCellCountAfterAck".to_string(),
                    format!("{}", self.charged_count(state)),
                );
                p.insert(
                    "rocketAfterAck".to_string(),
                    format!("{}", state.has_rocket()),
                );

                log.payload = p;
                log.emit();

                Some(PlanetToOrchestrator::SunrayAck {
                    planet_id: state.id(),
                })
            }
            OrchestratorToPlanet::InternalStateRequest { .. } => match self.rocket_strategy {
                RocketStrategy::EmergencyReserve => {
                    let mut dummy_state = PlanetState::to_dummy(state);

                    let mut p = Payload::new();
                    p.insert("type".to_string(), "InternalStateResponse".to_string());
                    p.insert(
                        "internalDummyState".to_string(),
                        format!("{:?}", dummy_state.clone()),
                    );
                    let mut log = LogEvent::new(
                        ActorType::Planet,
                        state.id(),
                        ActorType::Orchestrator,
                        0u32.to_string(),
                        EventType::MessagePlanetToOrchestrator,
                        Channel::Trace,
                        Payload::new(), //fake payload
                    );

                    dummy_state.charged_cells_count =
                        dummy_state.charged_cells_count.saturating_sub(1);

                    p.insert("sentDummyState".to_string(), format!("{:?}", dummy_state));
                    log.payload = p;
                    log.emit();

                    Some(PlanetToOrchestrator::InternalStateResponse {
                        planet_id: state.id(),
                        planet_state: dummy_state,
                    })
                }
                _ => {
                    let mut p = Payload::new();
                    p.insert("type".to_string(), "InternalStateResponse".to_string());
                    p.insert(
                        "DummyState".to_string(),
                        format!("{:?}", PlanetState::to_dummy(state)),
                    );
                    let log = LogEvent::new(
                        ActorType::Planet,
                        state.id(),
                        ActorType::Orchestrator,
                        0u32.to_string(),
                        EventType::MessagePlanetToOrchestrator,
                        Channel::Trace,
                        p,
                    );
                    log.emit();

                    Some(PlanetToOrchestrator::InternalStateResponse {
                        planet_id: state.id(),
                        planet_state: PlanetState::to_dummy(state),
                    })
                }
            },
            //OrchestratorToPlanet::Asteroid(_) => {}//handle_asteroid
            // OrchestratorToPlanet::StartPlanetAI(_) => {}//start
            // OrchestratorToPlanet::StopPlanetAI(_) => {}//stop
            _ => None,
        }
    }

    fn handle_explorer_msg(
        &mut self,
        state: &mut PlanetState,
        generator: &Generator,
        combinator: &Combinator,
        msg: ExplorerToPlanet,
    ) -> Option<PlanetToExplorer> {
        match msg {
            ExplorerToPlanet::SupportedResourceRequest { explorer_id } => {
                let mut p = Payload::new();
                p.insert("type".to_string(), "SupportedResourceResponse".to_string());
                p.insert(
                    "Recipes".to_string(),
                    format!("{:?}", generator.all_available_recipes()),
                );
                let log = LogEvent::new(
                    ActorType::Planet,
                    state.id(),
                    ActorType::Explorer,
                    explorer_id.to_string(),
                    EventType::MessagePlanetToExplorer,
                    Channel::Trace,
                    p,
                );
                log.emit();

                Some(PlanetToExplorer::SupportedResourceResponse {
                    resource_list: generator.all_available_recipes(),
                })
            }
            ExplorerToPlanet::SupportedCombinationRequest { explorer_id } => {
                let mut p = Payload::new();
                p.insert(
                    "type".to_string(),
                    "SupportedCombinationResponse".to_string(),
                );
                p.insert(
                    "Recipes".to_string(),
                    format!("{:?}", combinator.all_available_recipes()),
                );
                let log = LogEvent::new(
                    ActorType::Explorer,
                    explorer_id,
                    ActorType::Planet,
                    state.id().to_string(),
                    EventType::MessagePlanetToExplorer,
                    Channel::Trace,
                    p,
                );
                log.emit();

                Some(PlanetToExplorer::SupportedCombinationResponse {
                    combination_list: combinator.all_available_recipes(),
                })
            }
            ExplorerToPlanet::GenerateResourceRequest {
                explorer_id,
                resource,
            } => {
                let mut p = Payload::new();
                p.insert("type".to_string(), "GenerateResourceResponse".to_string());
                p.insert("ResourceRequested".to_string(), format!("{:?}", resource));
                p.insert(
                    "rocketStrategy".to_string(),
                    self.rocket_strategy.to_string(),
                );

                let mut log = LogEvent::new(
                    ActorType::Planet,
                    state.id(),
                    ActorType::Planet,
                    explorer_id.to_string(),
                    EventType::MessagePlanetToExplorer,
                    Channel::Debug,
                    Payload::new(),
                );

                if self.rocket_strategy == RocketStrategy::EmergencyReserve
                    && self.charged_count(state) <= 1
                {
                    p.insert(
                        "energyCellCount".to_string(),
                        format!("{} , this is intended behavior", self.charged_count(state)),
                    );
                    p.insert("Result".to_string(), "Failure".to_string());
                    log.payload = p;
                    log.emit();
                    return None;
                }
                let Some((cell, _)) = state.full_cell() else {
                    p.insert("Result".to_string(), "Failure".to_string());
                    log.payload = p;
                    log.emit();
                    return None;
                };
                //1- check the planet internal resource
                match self.basic_resource {
                    //2- check if the explorer is in fact asking for that one
                    BasicResourceType::Oxygen => match resource {
                        BasicResourceType::Oxygen => {
                            let new_basic_resource =
                                generator.make_oxygen(cell).ok().map(BasicResource::Oxygen);

                            p.insert("Result".to_string(), "Success".to_string());
                            log.payload = p;
                            log.emit();

                            Some(PlanetToExplorer::GenerateResourceResponse {
                                resource: new_basic_resource,
                            })
                        }

                        _ => {
                            p.insert("Result".to_string(), "Failure".to_string());
                            log.payload = p;
                            log.channel = Channel::Warning;
                            log.emit();
                            None
                        }
                    },
                    BasicResourceType::Hydrogen => match resource {
                        BasicResourceType::Hydrogen => {
                            let new_basic_resource = generator
                                .make_hydrogen(cell)
                                .ok()
                                .map(BasicResource::Hydrogen);

                            p.insert("Result".to_string(), "Success".to_string());
                            log.payload = p;
                            log.emit();

                            Some(PlanetToExplorer::GenerateResourceResponse {
                                resource: new_basic_resource,
                            })
                        }

                        _ => {
                            p.insert("Result".to_string(), "Failure".to_string());
                            log.payload = p;
                            log.channel = Channel::Warning;
                            log.emit();
                            None
                        }
                    },
                    BasicResourceType::Carbon => match resource {
                        BasicResourceType::Carbon => {
                            let new_basic_resource =
                                generator.make_carbon(cell).ok().map(BasicResource::Carbon);

                            p.insert("Result".to_string(), "Success".to_string());
                            log.payload = p;
                            log.emit();

                            Some(PlanetToExplorer::GenerateResourceResponse {
                                resource: new_basic_resource,
                            })
                        }

                        _ => {
                            p.insert("Result".to_string(), "Failure".to_string());
                            log.payload = p;
                            log.channel = Channel::Warning;
                            log.emit();
                            None
                        }
                    },
                    BasicResourceType::Silicon => match resource {
                        BasicResourceType::Silicon => {
                            let new_basic_resource = generator
                                .make_silicon(cell)
                                .ok()
                                .map(BasicResource::Silicon);

                            p.insert("Result".to_string(), "Success".to_string());
                            log.payload = p;
                            log.emit();

                            Some(PlanetToExplorer::GenerateResourceResponse {
                                resource: new_basic_resource,
                            })
                        }

                        _ => {
                            p.insert("Result".to_string(), "Failure".to_string());
                            log.payload = p;
                            log.channel = Channel::Warning;
                            log.emit();
                            None
                        }
                    },
                }
            }
            ExplorerToPlanet::CombineResourceRequest { explorer_id, msg } => {
                let mut p = Payload::new();
                p.insert("type".to_string(), "CombineResourceResponse".to_string());
                p.insert("ResourceRequested".to_string(), format!("{:?}", msg));
                p.insert(
                    "rocketStrategy".to_string(),
                    self.rocket_strategy.to_string(),
                );
                p.insert("Result".to_string(), "Failure".to_string());
                let log = LogEvent::new(
                    ActorType::Planet,
                    state.id(),
                    ActorType::Planet,
                    explorer_id.to_string(),
                    EventType::MessagePlanetToExplorer,
                    Channel::Warning,
                    p,
                );
                log.emit();

                None //type C doesn't combine

                //     let Some((cell, _)) = state.full_cell() else {
                //         return None;
                //     };
                //
                //     match msg {
                //         ComplexResourceRequest::Water(h, o) => {
                //             let new_complex_resource = combinator
                //                 .make_water(h, o, cell)
                //                 .map(ComplexResource::Water)
                //                 .map_err(|(msg, h, o)| {
                //                     (
                //                         msg,
                //                         GenericResource::BasicResources(BasicResource::Hydrogen(h)),
                //                         GenericResource::BasicResources(BasicResource::Oxygen(o)),
                //                     )
                //                 });
                //
                //             Some(PlanetToExplorer::CombineResourceResponse {
                //                 complex_response: new_complex_resource,
                //             })
                //         }
                //         ComplexResourceRequest::Diamond(c1, c2) => {
                //             let new_complex_resource = combinator
                //                 .make_diamond(c1, c2, cell)
                //                 .map(ComplexResource::Diamond)
                //                 .map_err(|(msg, c1, c2)| {
                //                     (
                //                         msg,
                //                         GenericResource::BasicResources(BasicResource::Carbon(c1)),
                //                         GenericResource::BasicResources(BasicResource::Carbon(c2)),
                //                     )
                //                 });
                //
                //             Some(PlanetToExplorer::CombineResourceResponse {
                //                 complex_response: new_complex_resource,
                //             })
                //         }
                //         ComplexResourceRequest::Life(w, c) => {
                //             let new_complex_resource = combinator
                //                 .make_life(w, c, cell)
                //                 .map(ComplexResource::Life)
                //                 .map_err(|(msg, w, c)| {
                //                     (
                //                         msg,
                //                         GenericResource::ComplexResources(ComplexResource::Water(w)),
                //                         GenericResource::BasicResources(BasicResource::Carbon(c)),
                //                     )
                //                 });
                //
                //             Some(PlanetToExplorer::CombineResourceResponse {
                //                 complex_response: new_complex_resource,
                //             })
                //         }
                //         ComplexResourceRequest::Robot(s, l) => {
                //             let new_complex_resource = combinator
                //                 .make_robot(s, l, cell)
                //                 .map(ComplexResource::Robot)
                //                 .map_err(|(msg, s, l)| {
                //                     (
                //                         msg,
                //                         GenericResource::BasicResources(BasicResource::Silicon(s)),
                //                         GenericResource::ComplexResources(ComplexResource::Life(l)),
                //                     )
                //                 });
                //
                //             Some(PlanetToExplorer::CombineResourceResponse {
                //                 complex_response: new_complex_resource,
                //             })
                //         }
                //         ComplexResourceRequest::Dolphin(w, l) => {
                //             let new_complex_resource = combinator
                //                 .make_dolphin(w, l, cell)
                //                 .map(ComplexResource::Dolphin)
                //                 .map_err(|(msg, w, l)| {
                //                     (
                //                         msg,
                //                         GenericResource::ComplexResources(ComplexResource::Water(w)),
                //                         GenericResource::ComplexResources(ComplexResource::Life(l)),
                //                     )
                //                 });
                //
                //             Some(PlanetToExplorer::CombineResourceResponse {
                //                 complex_response: new_complex_resource,
                //             })
                //         }
                //         ComplexResourceRequest::AIPartner(r, d) => {
                //             let new_complex_resource = combinator
                //                 .make_aipartner(r, d, cell)
                //                 .map(ComplexResource::AIPartner)
                //                 .map_err(|(msg, r, d)| {
                //                     (
                //                         msg,
                //                         GenericResource::ComplexResources(ComplexResource::Robot(r)),
                //                         GenericResource::ComplexResources(ComplexResource::Diamond(d)),
                //                     )
                //                 });
                //
                //             Some(PlanetToExplorer::CombineResourceResponse {
                //                 complex_response: new_complex_resource,
                //             })
                //         }
                //     }
            }
            ExplorerToPlanet::AvailableEnergyCellRequest { explorer_id } => {
                let count = self.charged_count(state) ;

                let mut p = Payload::new();
                p.insert("type".to_string(), "AvailableEnergyCellResponse".to_string());
                p.insert(
                    "internalEnergyCellCount".to_string(),
                    format!("{:?}", count),
                );
                p.insert(
                    "rocketStrategy".to_string(),
                    self.rocket_strategy.to_string(),
                );

                let available_cells = match self.rocket_strategy {
                    RocketStrategy::EmergencyReserve => count.saturating_sub(1) as u32,
                    _ => count as u32,
                };

                p.insert("sentEnergyCellCount".to_string(), format!("{:?}", count));

                p.insert("Result".to_string(), "Failure".to_string());
                let log = LogEvent::new(
                    ActorType::Planet,
                    state.id(),
                    ActorType::Planet,
                    explorer_id.to_string(),
                    EventType::MessagePlanetToExplorer,
                    Channel::Trace,
                    p,
                );
                log.emit();

                Some(PlanetToExplorer::AvailableEnergyCellResponse { available_cells })
            }
        }
    }

    fn handle_asteroid(
        &mut self,
        state: &mut PlanetState,
        _generator: &Generator,
        _combinator: &Combinator,
    ) -> Option<Rocket> {
        let mut p = Payload::new();
        p.insert("type".to_string(), "AsteroidAck".to_string());
        p.insert("HadRocket".to_string(), format!("{:?}", state.has_rocket()));
        p.insert(
            "rocketStrategy".to_string(),
            self.rocket_strategy.to_string(),
        );
        let mut log = LogEvent::new(
            ActorType::Planet,
            state.id(),
            ActorType::Orchestrator,
            0u32.to_string(),
            EventType::MessagePlanetToOrchestrator,
            Channel::Info,
            Payload::new(),
        );

        if !state.can_have_rocket() {
            log.payload = p;
            log.emit();
            return None;
        }
        if self.rocket_strategy == RocketStrategy::Default {
            let result = try_build_rocket(state);
            if result.is_some() {
                p.insert(
                    "Built a Rocket, energyCellCount".to_string(),
                    format!("{:?}", self.charged_count(state)),
                );
            }
        }
        if !state.has_rocket() {
            log.payload = p;
            log.emit();
            return None;
        }

        let rocket = state.take_rocket();
        if self.rocket_strategy == RocketStrategy::Safe
            || self.rocket_strategy == RocketStrategy::EmergencyReserve
        {
            let result = try_build_rocket(state);
            if result.is_some() {
                p.insert(
                    "Built a Rocket, energyCellCount".to_string(),
                    format!("{:?}", self.charged_count(state)),
                );
            }
        }
        log.payload = p;
        log.emit();
        rocket
    }

    fn start(&mut self, state: &PlanetState) {
        let mut p = Payload::new();
        p.insert("type".to_string(), "StartAI".to_string());
        LogEvent::new(
            ActorType::Planet,
            state.id(),
            ActorType::SelfActor,
            0u32.to_string(),
            EventType::InternalPlanetAction,
            Channel::Info,
            p,
        )
        .emit();

        self.running = true;
    }

    fn stop(&mut self, state: &PlanetState) {
        let mut p = Payload::new();
        p.insert("type".to_string(), "StopAI".to_string());
        LogEvent::new(
            ActorType::Planet,
            state.id(),
            ActorType::SelfActor,
            0u32.to_string(),
            EventType::InternalPlanetAction,
            Channel::Info,
            p,
        )
        .emit();

        self.running = false;
    }
}

/// Tries to build a rocket using the first fully charged energy cell.
/// Returns `Some(index)` on success, or `None` on failure.
///
/// This helper extracts a full cell through `state.full_cell()`, which provides
/// both the mutable reference and its index. If no full cell exists or the
/// rocket cannot be built, the function returns `None`.
fn try_build_rocket(state: &mut PlanetState) -> Option<usize> {
    let Some((_, cell_index)) = state.full_cell() else {
        return None;
    };
    state.build_rocket(cell_index).ok()?; // if Err -> return None

    Some(cell_index)
}

/// Creates and initializes a new `Planet` instance with a predefined set of
/// generation and combination rules, a basic AI model, and the communication
/// channels used to interact with the orchestrator and explorers.
///
/// Planet configuration
/// - Type: C
/// - Generation rule: Oxygen
/// - Combination rules: Diamond, Water, Life, Robot, Dolphin, AIPartner
///
/// Parameters
/// - The channels used to receive messages from the orchestrator and
///   send responses back
/// - The channel used to receive messages from explorers
/// - planet_id: the id of the planet
/// - rocket_strategy: takes an Option<BasicResourceType> where BasicResourceType is an Enum containing:
///     - Disabled: do not generate rockets under any condition.
///     - Default: generate a rocket only when an asteroid is coming.
///     - Safe: always rebuild a rocket when there isn't any
///     - EmergencyReserve: same as `Safe`, but preserves one fully charged cell for emergencies.
/// - basic_resource: takes an Option<BasicResourceType> and set that one as a basic resource for the planet
///
/// Returns:
/// - `Ok(Planet)` if the configuration is valid for the selected planet type
/// - `Err(String)` if the rules exceed the constraints of the planet type
pub fn new_planet(
    rx_orchestrator: Receiver<OrchestratorToPlanet>,
    tx_orchestrator: Sender<PlanetToOrchestrator>,
    rx_explorer: Receiver<ExplorerToPlanet>,
    planet_id: u32,
    rocket_strategy: RocketStrategy,
    basic_resource: Option<BasicResourceType>,
) -> Result<Planet, String> {
    let gen_rules = if let Some(b_res) = basic_resource {
        vec![b_res]
    } else {
        vec![
            // BasicResourceType::Oxygen,
            BasicResourceType::Hydrogen,
            // BasicResourceType::Carbon,
            // BasicResourceType::Silicon,
        ]
    };

    let comb_rules = vec![
        // ComplexResourceType::Diamond,
        // ComplexResourceType::Water,
        // ComplexResourceType::Life,
        // ComplexResourceType::Robot,
        // ComplexResourceType::Dolphin,
        // ComplexResourceType::AIPartner,
    ];
    let ai = PlanetCoreThinkingModel {
        rocket_strategy : rocket_strategy.clone(),
        running: false,
        basic_resource: basic_resource.unwrap_or(BasicResourceType::Hydrogen),
    };


    let mut p = Payload::new();
    p.insert("type".to_string(), "Creation".to_string());
    p.insert("planetId".to_string(), planet_id.to_string());
    p.insert("basicResourceRule".to_string(), format!("{:?}", basic_resource.unwrap_or(BasicResourceType::Hydrogen)));
    p.insert("planetType".to_string(), format!("{:?}",PlanetType::A));
    p.insert("rocketStrategy".to_string(), format!("{:?}",rocket_strategy));
    LogEvent::new(
        ActorType::Planet,
        planet_id,
        ActorType::SelfActor,
        0u32.to_string(),
        EventType::InternalPlanetAction,
        Channel::Info,
        p,
    ).emit();

    Planet::new(
        planet_id,
        PlanetType::A,
        Box::new(ai),
        gen_rules,
        comb_rules,
        (rx_orchestrator, tx_orchestrator),
        rx_explorer,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use common_game::components::forge::Forge;
    use common_game::components::resource::BasicResourceType;
    use common_game::protocols::messages::{
        ExplorerToPlanet, OrchestratorToPlanet, PlanetToExplorer, PlanetToOrchestrator,
    };
    use crossbeam_channel::{unbounded, Receiver, Sender};
    use std::sync::OnceLock;
    use std::thread;
    use std::time::Duration;

    // --- Safe Singleton Helper for Forge ---
    static FORGE: OnceLock<Forge> = OnceLock::new();

    fn get_forge() -> &'static Forge {
        FORGE.get_or_init(|| {
            Forge::new().expect("Failed to initialize Forge singleton")
        })
    }

    // --- Test Harness ---
    // This helper spawns the planet thread and returns the channels to talk to it.
    fn spawn_test_planet(
        strategy: RocketStrategy,
        resource: BasicResourceType,
    ) -> (
        Sender<OrchestratorToPlanet>,
        Receiver<PlanetToOrchestrator>,
        Sender<ExplorerToPlanet>,
        Receiver<PlanetToExplorer>,
    ) {
        // 1. Create Channels
        let (orch_tx, orch_rx) = unbounded();          // Test -> Planet (Orch)
        let (planet_to_orch_tx, planet_to_orch_rx) = unbounded(); // Planet -> Test (Orch)

        let (expl_tx, expl_rx) = unbounded();          // Test -> Planet (Expl)
        // We need a channel to receive Explorer responses.
        // We will inject this via the Handshake message.
        let (test_expl_response_tx, test_expl_response_rx) = unbounded();

        // 2. Instantiate Planet using your helper function `new_planet`
        let mut planet = new_planet(
            orch_rx,
            planet_to_orch_tx,
            expl_rx,
            1, // Planet ID
            strategy,
            Some(resource),
        ).expect("Failed to create planet instance");

        // 3. Run Planet in Background Thread
        thread::spawn(move || {
            // run() blocks until the planet is killed
            let _ = planet.run();
        });

        // 4. Start the AI
        orch_tx.send(OrchestratorToPlanet::StartPlanetAI).unwrap();
        // Wait for Start Result (consume the message)
        let _ = planet_to_orch_rx.recv().unwrap();

        // 5. Register our Test Explorer (Handshake)
        orch_tx.send(OrchestratorToPlanet::IncomingExplorerRequest {
            explorer_id: 99,
            new_mpsc_sender: test_expl_response_tx
        }).unwrap();
        // Wait for Handshake Ack
        let _ = planet_to_orch_rx.recv().unwrap();

        (orch_tx, planet_to_orch_rx, expl_tx, test_expl_response_rx)
    }

    // ==========================================
    // TESTS
    // ==========================================

    #[test]
    fn test_strategy_safe_builds_rocket_immediately() {
        // SCENARIO: Safe strategy should build a rocket immediately after receiving energy.
        let forge = get_forge();
        let (orch_tx, orch_rx, _, _) = spawn_test_planet(RocketStrategy::Safe, BasicResourceType::Hydrogen);

        // 1. Send Sunray
        orch_tx.send(OrchestratorToPlanet::Sunray(forge.generate_sunray())).unwrap();

        // 2. Wait for SunrayAck
        let ack = orch_rx.recv_timeout(Duration::from_secs(1)).expect("Timeout waiting for SunrayAck");

        // 3. Verify Internal State
        // Since we can't inspect PlanetState directly, we ask the planet for its state.
        orch_tx.send(OrchestratorToPlanet::InternalStateRequest).unwrap();

        let state_msg = orch_rx.recv_timeout(Duration::from_secs(1)).expect("Timeout waiting for State");

        if let PlanetToOrchestrator::InternalStateResponse { planet_state, .. } = state_msg {
            // The 'Safe' strategy logic is: If I have energy, make a rocket.
            assert!(planet_state.has_rocket, "Safe strategy failed to build rocket immediately");
        } else {
            panic!("Unexpected response type: no debug trait "); //{:?}",  state_msg);
        }
    }

    #[test]
    fn test_strategy_default_waits_for_asteroid() {
        // SCENARIO: Default strategy keeps energy stored and only builds when threatened.
        let forge = get_forge();
        let (orch_tx, orch_rx, _, _) = spawn_test_planet(RocketStrategy::Default, BasicResourceType::Hydrogen);

        // 1. Send Sunray
        orch_tx.send(OrchestratorToPlanet::Sunray(forge.generate_sunray())).unwrap();
        let _ = orch_rx.recv(); // Consume Ack

        // 2. Verify NO Rocket yet
        orch_tx.send(OrchestratorToPlanet::InternalStateRequest).unwrap();
        let state_msg = orch_rx.recv().unwrap();

        if let PlanetToOrchestrator::InternalStateResponse { planet_state, .. } = state_msg {
            assert!(!planet_state.has_rocket, "Default strategy built rocket too early!");
            assert!(planet_state.charged_cells_count > 0, "Default strategy lost the energy!");
        }

        // 3. Send Asteroid
        orch_tx.send(OrchestratorToPlanet::Asteroid(forge.generate_asteroid())).unwrap();

        // 4. Expect Rocket Launch (AsteroidAck with Rocket)
        let ack = orch_rx.recv_timeout(Duration::from_secs(1)).expect("Timeout waiting for AsteroidAck");

        if let PlanetToOrchestrator::AsteroidAck { rocket, .. } = ack {
            assert!(rocket.is_some(), "Default strategy failed to build rocket for asteroid");
        } else {
            panic!("Wrong message type received for Asteroid");
        }
    }

    #[test]
    fn test_emergency_reserve_deception() {
        // SCENARIO: EmergencyReserve keeps 1 cell hidden.
        // If we only give it 1 Sunray, it should claim to be empty.
        let forge = get_forge();
        let (orch_tx, orch_rx, expl_tx, expl_rx) = spawn_test_planet(RocketStrategy::EmergencyReserve, BasicResourceType::Hydrogen);

        // 1. Charge exactly 1 cell
        orch_tx.send(OrchestratorToPlanet::Sunray(forge.generate_sunray())).unwrap();
        let _ = orch_rx.recv(); // Consume Ack

        // 2. Check Orchestrator Report (The "Lie")
        orch_tx.send(OrchestratorToPlanet::InternalStateRequest).unwrap();
        let state_msg = orch_rx.recv().unwrap();

        if let PlanetToOrchestrator::InternalStateResponse { planet_state, .. } = state_msg {
            // Real state is 1, but logic subtracts 1.
            assert_eq!(planet_state.charged_cells_count, 0, "EmergencyReserve failed to hide the reserve cell from Orchestrator");
        }

        // 3. Check Explorer Availability (The "Denial")
        expl_tx.send(ExplorerToPlanet::AvailableEnergyCellRequest { explorer_id: 99 }).unwrap();
        let expl_resp = expl_rx.recv().unwrap();

        if let PlanetToExplorer::AvailableEnergyCellResponse { available_cells } = expl_resp {
            assert_eq!(available_cells, 0, "EmergencyReserve failed to hide reserve cell from Explorer");
        }

        // 4. Verify Resource Generation fails
        expl_tx.send(ExplorerToPlanet::GenerateResourceRequest {
            explorer_id: 99,
            resource: BasicResourceType::Hydrogen
        }).unwrap();

        // We expect no success response because the logic returns `None` when hitting the reserve.
        let result = expl_rx.recv_timeout(Duration::from_millis(200));
        assert!(result.is_err(), "Planet should not generate resources using the emergency reserve");
    }

    #[test]
    fn test_resource_generation_match() {
        // SCENARIO: Planet is set to produce Oxygen. Request Oxygen (Success) then Carbon (Failure).
        let forge = get_forge();
        let (orch_tx, orch_rx, expl_tx, expl_rx) = spawn_test_planet(RocketStrategy::Default, BasicResourceType::Oxygen);

        // 1. Charge Up
        orch_tx.send(OrchestratorToPlanet::Sunray(forge.generate_sunray())).unwrap();
        let _ = orch_rx.recv();

        // 2. Request Correct Resource (Oxygen)
        expl_tx.send(ExplorerToPlanet::GenerateResourceRequest {
            explorer_id: 99,
            resource: BasicResourceType::Oxygen
        }).unwrap();

        let resp = expl_rx.recv_timeout(Duration::from_secs(1)).expect("Should generate Oxygen");
        match resp {
            PlanetToExplorer::GenerateResourceResponse { resource: Some(_) } => assert!(true),
            _ => panic!("Failed to generate correct resource"),
        }

        // 3. Recharge (assume previous request used the energy)
        orch_tx.send(OrchestratorToPlanet::Sunray(forge.generate_sunray())).unwrap();
        let _ = orch_rx.recv();

        // 4. Request Incorrect Resource (Carbon)
        expl_tx.send(ExplorerToPlanet::GenerateResourceRequest {
            explorer_id: 99,
            resource: BasicResourceType::Carbon
        }).unwrap();

        // Should return None (impl logic) -> Timeout on channel
        let result = expl_rx.recv_timeout(Duration::from_millis(200));
        assert!(result.is_err(), "Planet generated a resource it does not support!");
    }
}