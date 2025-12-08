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
#[derive(Debug, PartialEq, Eq)]
pub enum RocketStrategy {
    /// Do not generate rockets under any condition.
    Disabled,

    /// Normal behavior: generate a rocket only when an asteroid is coming.
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

impl PlanetAI for PlanetCoreThinkingModel {
    fn handle_orchestrator_msg(
        &mut self,
        state: &mut PlanetState,
        generator: &Generator,
        combinator: &Combinator,
        msg: OrchestratorToPlanet,
    ) -> Option<PlanetToOrchestrator> {
        match msg {
            OrchestratorToPlanet::Sunray(sunray) => {
                let mut p = Payload::new();
                p.insert("type".to_string(), "Sunray".to_string());
                p.insert(
                    "rocketStrategy".to_string(),
                    self.rocket_strategy.to_string(),
                );
                p.insert(
                    "energyCellCountBeforeAck".to_string(),
                    format!("{}", state.cells_count()),
                );
                p.insert(
                    "rocketBeforeAck".to_string(),
                    format!("{}", state.has_rocket()),
                );
                let mut log = LogEvent::new(
                    ActorType::Orchestrator,
                    0u32,
                    ActorType::Planet,
                    state.id().to_string(),
                    EventType::MessageOrchestratorToPlanet,
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
                }
                else {
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
                    format!("{}", state.cells_count()),
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
                    p.insert("type".to_string(), "InternalStateRequest".to_string());
                    p.insert(
                        "internalDummyState".to_string(),
                        format!("{:?}", dummy_state.clone()),
                    );
                    let mut log = LogEvent::new(
                        ActorType::Orchestrator,
                        0u32,
                        ActorType::Planet,
                        state.id().to_string(),
                        EventType::MessageOrchestratorToPlanet,
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
                    p.insert("type".to_string(), "InternalStateRequest".to_string());
                    p.insert(
                        "DummyState".to_string(),
                        format!("{:?}", PlanetState::to_dummy(state)),
                    );
                    let log = LogEvent::new(
                        ActorType::Orchestrator,
                        0u32,
                        ActorType::Planet,
                        state.id().to_string(),
                        EventType::MessageOrchestratorToPlanet,
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
                p.insert("type".to_string(), "SupportedResourceRequest".to_string());
                p.insert(
                    "Recipes".to_string(),
                    format!("{:?}", generator.all_available_recipes()),
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

                Some(PlanetToExplorer::SupportedResourceResponse {
                    resource_list: generator.all_available_recipes(),
                })
            }
            ExplorerToPlanet::SupportedCombinationRequest { explorer_id } => {
                let mut p = Payload::new();
                p.insert(
                    "type".to_string(),
                    "SupportedCombinationRequest".to_string(),
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
                p.insert("type".to_string(), "GenerateResourceRequest".to_string());
                p.insert("ResourceRequested".to_string(), format!("{:?}", resource));
                p.insert(
                    "rocketStrategy".to_string(),
                    self.rocket_strategy.to_string(),
                );

                let mut log = LogEvent::new(
                    ActorType::Explorer,
                    explorer_id,
                    ActorType::Planet,
                    state.id().to_string(),
                    EventType::MessagePlanetToExplorer,
                    Channel::Debug,
                    Payload::new(),
                );

                if self.rocket_strategy == RocketStrategy::EmergencyReserve
                    && state.cells_count() <= 1
                {
                    p.insert(
                        "energyCellCount".to_string(),
                        format!("{} , this is intended behavior", state.cells_count()),
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
                            log.emit();
                            None
                        }
                    },
                }
            }
            ExplorerToPlanet::CombineResourceRequest {
                explorer_id,
                msg,
            } => {
                let mut p = Payload::new();
                p.insert("type".to_string(), "CombineResourceRequest".to_string());
                p.insert("ResourceRequested".to_string(), format!("{:?}", msg));
                p.insert(
                    "rocketStrategy".to_string(),
                    self.rocket_strategy.to_string(),
                );
                p.insert("Result".to_string(), "Failure".to_string());
                let log = LogEvent::new(
                    ActorType::Explorer,
                    explorer_id,
                    ActorType::Planet,
                    state.id().to_string(),
                    EventType::MessagePlanetToExplorer,
                    Channel::Debug,
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

                let count = state.cells_count();

                let mut p = Payload::new();
                p.insert("type".to_string(), "AvailableEnergyCellRequest".to_string());
                p.insert("internalEnergyCellCount".to_string(), format!("{:?}", count));
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
                    ActorType::Explorer,
                    explorer_id,
                    ActorType::Planet,
                    state.id().to_string(),
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
        generator: &Generator,
        combinator: &Combinator,
    ) -> Option<Rocket> {
        if !state.can_have_rocket() {
            return None;
        }
        if self.rocket_strategy == RocketStrategy::Default {
            let _ = try_build_rocket(state);
            println!("Building default Rocket");
        }
        if !state.has_rocket() {
            return None;
        }
        let rocket = state.take_rocket();
        if self.rocket_strategy == RocketStrategy::Safe
            || self.rocket_strategy == RocketStrategy::EmergencyReserve
        {
            let _ = try_build_rocket(state);
        }
        rocket
    }

    fn start(&mut self, state: &PlanetState) {
        self.running = true;
    }

    fn stop(&mut self, state: &PlanetState) {
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
        rocket_strategy,
        running: false,
        basic_resource: basic_resource.unwrap_or(BasicResourceType::Hydrogen),
    };

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
