use crate::{
    error::{CommandError, CommandResult},
    models::{GenerationOptions, MotionPhase, MotionPlan},
};

#[derive(Clone, Copy)]
struct PhaseDefinition {
    name: &'static str,
    description: &'static str,
    weight: f64,
}

fn contains_word(text: &str, expected: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word == expected)
}

fn explicit_frame_count(prompt: &str) -> Option<u32> {
    let words: Vec<_> = prompt
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    words.windows(2).find_map(|pair| {
        (pair[1].eq_ignore_ascii_case("frame") || pair[1].eq_ignore_ascii_case("frames"))
            .then(|| pair[0].parse().ok())
            .flatten()
    })
}

fn classify_motion(prompt: &str) -> (&'static str, u32, Vec<PhaseDefinition>, bool, bool) {
    let lower = prompt.to_ascii_lowercase();
    if ["death", "dying", "collapse", "defeat"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a readable one-shot death",
            10,
            vec![
                phase("Alive", "Establish the final controlled pose", 0.8),
                phase("Reaction", "Show the source and direction of force", 0.8),
                phase(
                    "Loss of balance",
                    "Break the standing center of gravity",
                    0.9,
                ),
                phase("Descent", "Carry the silhouette toward the ground", 0.8),
                phase("Impact", "Show the body reaching the ground", 1.0),
                phase(
                    "Compression",
                    "Absorb the impact without changing identity",
                    0.8,
                ),
                phase("Settle", "Resolve secondary motion", 1.0),
                phase("Rest", "Hold the final readable state", 1.3),
            ],
            false,
            false,
        );
    }
    if ["dodge", "roll", "evade", "sidestep"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a fast evasive action",
            6,
            vec![
                phase(
                    "Ready",
                    "NEAR and FAR legs grounded shoulder width; establish the starting pose",
                    0.8,
                ),
                phase(
                    "Anticipation",
                    "Compress opposite the escape direction; NEAR leg loads",
                    0.8,
                ),
                phase(
                    "Launch",
                    "NEAR leg pushes the center of mass into the dodge while the FAR leg trails",
                    0.6,
                ),
                phase(
                    "Travel",
                    "Show the clearest evasive silhouette; FAR leg tucks behind the NEAR leg",
                    0.6,
                ),
                phase(
                    "Landing",
                    "FAR leg reconnects to the ground first and absorbs the landing",
                    0.8,
                ),
                phase(
                    "Recovery",
                    "Both legs regrounded under the body in a controllable pose",
                    1.0,
                ),
            ],
            false,
            true,
        );
    }
    if contains_word(&lower, "cast")
        || contains_word(&lower, "casting")
        || contains_word(&lower, "spell")
    {
        return (
            "a staged spell cast",
            9,
            vec![
                phase("Ready", "Establish the caster and focus", 0.9),
                phase("Anticipation", "Draw the hands or focus inward", 0.8),
                phase("Gather", "Build readable magical energy", 1.0),
                phase("Charge", "Increase energy and secondary motion", 1.0),
                phase("Release", "Show the decisive casting gesture", 0.7),
                phase("Flash", "Hold the clearest effect contact", 0.8),
                phase("Follow-through", "Carry the gesture past release", 0.8),
                phase("Recovery", "Settle character and effect remnants", 1.0),
                phase("Return", "Reach the final stable pose", 0.9),
            ],
            false,
            false,
        );
    }
    let heavy = ["heavy", "greatsword", "spinning", "combo"]
        .iter()
        .any(|word| contains_word(&lower, word));
    if heavy {
        return (
            "a multi-stage heavy action",
            12,
            vec![
                phase("Start", "Establish the readable starting pose", 0.8),
                phase("Anticipation", "Shift weight opposite the action", 1.1),
                phase("Wind-up", "Load the body and weapon", 1.2),
                phase("Acceleration", "Begin the committed movement", 0.8),
                phase("Primary action", "Carry the fastest readable arc", 0.7),
                phase("Impact", "Show contact and maximum force", 1.1),
                phase("Secondary action", "Continue the combo or spin", 0.9),
                phase("Follow-through", "Resolve momentum", 1.0),
                phase("Recovery", "Return balance and silhouette", 1.1),
                phase("Return", "Reconnect cleanly to the start", 0.8),
            ],
            false,
            false,
        );
    }
    if ["walk", "walking"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a looping walk cycle",
            8,
            vec![
                phase(
                    "Near contact",
                    "NEAR leg plants ahead under the hip at heel strike; FAR leg trails behind on the toe",
                    1.0,
                ),
                phase(
                    "Near down",
                    "Weight compresses over the NEAR planted foot; FAR heel lifts",
                    0.9,
                ),
                phase(
                    "Near passing",
                    "FAR leg swings through beside the NEAR planted leg; the NEAR leg occludes at overlap",
                    0.8,
                ),
                phase(
                    "Near up",
                    "Body rises over the NEAR toe; FAR leg reaches ahead for its contact",
                    0.9,
                ),
                phase(
                    "Far contact",
                    "FAR leg plants ahead under the hip; NEAR leg trails behind on the toe",
                    1.0,
                ),
                phase(
                    "Far down",
                    "Weight compresses over the FAR planted foot; NEAR heel lifts",
                    0.9,
                ),
                phase(
                    "Far passing",
                    "NEAR leg swings through beside the FAR planted leg; the FAR leg occludes at overlap",
                    0.8,
                ),
                phase(
                    "Far up",
                    "Body rises over the FAR toe; NEAR leg reaches ahead to close the loop",
                    0.9,
                ),
            ],
            true,
            true,
        );
    }
    if ["run", "running", "sprint"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a looping run cycle",
            8,
            vec![
                phase(
                    "Near contact",
                    "NEAR leg reaches into contact under the hips; FAR leg trails behind with a bent knee",
                    0.8,
                ),
                phase(
                    "Near compression",
                    "NEAR stance leg absorbs and lowers the body; FAR leg folds and swings forward",
                    0.8,
                ),
                phase(
                    "Near drive",
                    "NEAR toe drives the body forward while the FAR knee drives through",
                    0.7,
                ),
                phase(
                    "Flight",
                    "Both feet airborne; FAR leg now reaching ahead while the NEAR leg trails",
                    0.7,
                ),
                phase(
                    "Far contact",
                    "FAR leg reaches into contact under the hips; NEAR leg trails behind with a bent knee",
                    0.8,
                ),
                phase(
                    "Far compression",
                    "FAR stance leg absorbs and lowers the body; NEAR leg folds and swings forward",
                    0.8,
                ),
                phase(
                    "Far drive",
                    "FAR toe drives the body forward while the NEAR knee drives through",
                    0.7,
                ),
                phase(
                    "Flight return",
                    "Both feet airborne again; NEAR leg reaches ahead to close the loop",
                    0.7,
                ),
            ],
            true,
            true,
        );
    }
    if ["idle", "breath", "breathing", "blink"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a subtle looping idle",
            5,
            vec![
                phase("Rest", "Hold the grounded neutral silhouette", 1.2),
                phase("Inhale", "Lift the torso or secondary forms slightly", 1.0),
                phase("Apex", "Reach the smallest readable high point", 0.9),
                phase("Exhale", "Ease back through neutral", 1.0),
                phase("Settle", "Reconnect to the opening pose", 1.2),
            ],
            true,
            false,
        );
    }
    if ["open", "opening", "close", "closing", "unfold"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a hinged or mechanical object action",
            8,
            vec![
                phase("Closed", "Establish the locked object body", 1.0),
                phase(
                    "Anticipation",
                    "Give the moving part a small preparatory shift",
                    0.8,
                ),
                phase(
                    "Early opening",
                    "Begin rotation around the fixed hinge",
                    0.8,
                ),
                phase("Opening", "Continue the connected mechanical arc", 0.8),
                phase("Reveal", "Reach the readable open state or payload", 1.1),
                phase("Hold", "Let the open result read at game scale", 1.2),
                phase("Closing", "Reverse through the same hinge path", 0.9),
                phase("Settle", "Return cleanly to the closed loop pose", 1.0),
            ],
            true,
            false,
        );
    }
    if ["attack", "slash", "strike", "swing", "shoot"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a one-shot attack",
            7,
            vec![
                phase("Start", "Establish the neutral combat silhouette", 0.8),
                phase(
                    "Anticipation",
                    "Make the direction of force readable; weight shifts onto the FAR leg",
                    1.0,
                ),
                phase("Wind-up", "Load the body or weapon over the grounded FAR leg", 1.0),
                phase(
                    "Acceleration",
                    "Move rapidly toward contact; NEAR leg starts its step",
                    0.7,
                ),
                phase(
                    "Impact",
                    "Decisive contact pose; NEAR foot planted, FAR heel lifted",
                    0.9,
                ),
                phase("Follow-through", "Carry momentum past impact", 0.9),
                phase("Recovery", "Return to a stable pose", 1.1),
            ],
            false,
            true,
        );
    }
    if ["explosion", "burst", "spark", "smoke", "effect", "vfx"]
        .iter()
        .any(|word| contains_word(&lower, word))
    {
        return (
            "a staged visual effect",
            8,
            vec![
                phase("Seed", "Establish the compact source", 0.5),
                phase("Ignition", "Create the first high-energy change", 0.6),
                phase("Expansion", "Grow the effect silhouette rapidly", 0.7),
                phase("Peak", "Reach maximum area and brightness", 0.8),
                phase(
                    "Breakup",
                    "Separate the main mass into readable fragments",
                    0.7,
                ),
                phase("Dissipation", "Reduce opacity and energy", 0.9),
                phase("Fade", "Leave only secondary traces", 1.0),
                phase(
                    "Clear",
                    "Return to transparent for a clean loop or finish",
                    0.6,
                ),
            ],
            false,
            false,
        );
    }
    (
        "a general readable motion",
        6,
        vec![
            phase("Start", "Establish the initial silhouette", 1.0),
            phase("Anticipation", "Prepare the primary change", 0.9),
            phase("Action", "Perform the main movement", 0.8),
            phase("Peak", "Show the clearest extreme pose", 1.0),
            phase("Recovery", "Resolve the movement", 0.9),
            phase("Settle", "Reach the final or looping pose", 1.0),
        ],
        lower.contains("loop"),
        false,
    )
}

const fn phase(name: &'static str, description: &'static str, weight: f64) -> PhaseDefinition {
    PhaseDefinition {
        name,
        description,
        weight,
    }
}

fn allocate_phases(definitions: Vec<PhaseDefinition>, frame_count: u32) -> Vec<MotionPhase> {
    if frame_count == 0 {
        return Vec::new();
    }
    let desired = frame_count as usize;
    let selected = if definitions.len() <= desired {
        definitions
    } else if desired == 1 {
        vec![definitions[definitions.len() / 2]]
    } else {
        (0..desired)
            .map(|index| {
                let source = index * (definitions.len() - 1) / (desired - 1);
                definitions[source]
            })
            .collect()
    };
    let mut phases: Vec<_> = selected
        .iter()
        .map(|definition| MotionPhase {
            name: definition.name.into(),
            description: definition.description.into(),
            frame_count: 1,
            timing_weight: definition.weight,
        })
        .collect();
    let mut remaining = desired.saturating_sub(phases.len());
    let mut order: Vec<_> = (0..phases.len()).collect();
    order.sort_by(|left, right| {
        phases[*right]
            .timing_weight
            .total_cmp(&phases[*left].timing_weight)
    });
    let mut index = 0;
    while remaining > 0 {
        phases[order[index % order.len()]].frame_count += 1;
        remaining -= 1;
        index += 1;
    }
    phases
}

pub fn build_motion_plan(
    prompt: &str,
    generation: &GenerationOptions,
) -> CommandResult<MotionPlan> {
    if !matches!(generation.frame_mode.as_str(), "fixed" | "auto") {
        return Err(CommandError::new(
            "invalid_frame_mode",
            "Frame mode must be Fixed or Auto",
        ));
    }
    let minimum = generation.min_frames.clamp(1, 64);
    let maximum = generation.max_frames.clamp(minimum, 64);
    let (motion_kind, recommended, definitions, inferred_loop, limb_identity) =
        classify_motion(prompt);
    let explicit = explicit_frame_count(prompt).filter(|count| (1..=64).contains(count));
    let single_frame = prompt.trim_start().starts_with("/sprite")
        || prompt.to_ascii_lowercase().contains("single frame")
        || prompt.to_ascii_lowercase().contains("one frame");
    let selected = if single_frame {
        1
    } else if let Some(explicit) = explicit {
        explicit
    } else if generation.frame_mode == "fixed" {
        generation.frames.clamp(1, 64)
    } else {
        recommended.clamp(minimum, maximum)
    };
    let selected = if prompt.trim_start().starts_with("/animate") && !single_frame {
        selected.max(2)
    } else {
        selected
    };
    let effective_mode = if explicit.is_some() || single_frame {
        "fixed"
    } else {
        generation.frame_mode.as_str()
    };
    let explanation = if single_frame {
        "A single frame was selected because the request is for a static sprite.".into()
    } else if explicit.is_some() || generation.frame_mode == "fixed" {
        format!(
            "Exactly {selected} frames will be used and the motion phases will be fitted to that limit."
        )
    } else {
        format!(
            "{selected} frames selected within the {minimum}–{maximum} limit because the request describes {motion_kind}."
        )
    };
    let explanation = if limb_identity {
        format!(
            "{explanation} Paired limbs are named by camera depth (NEAR limb and FAR limb), never by screen side; the FAR limb keeps a darker shade so the legs never swap identity."
        )
    } else {
        explanation
    };
    Ok(MotionPlan {
        frame_mode: effective_mode.into(),
        selected_frame_count: selected,
        minimum_frame_count: if effective_mode == "auto" {
            minimum
        } else {
            selected
        },
        maximum_frame_count: if effective_mode == "auto" {
            maximum
        } else {
            selected
        },
        fps: generation.fps,
        looping: inferred_loop,
        allow_interpolation: generation.allow_interpolation,
        allow_auto_adjust: generation.allow_auto_adjust,
        explanation,
        phases: allocate_phases(definitions, selected),
    })
}

#[tauri::command]
pub fn plan_motion(prompt: String, generation: GenerationOptions) -> CommandResult<MotionPlan> {
    build_motion_plan(prompt.trim(), &generation)
}

#[cfg(test)]
mod tests {
    use super::build_motion_plan;
    use crate::models::GenerationOptions;

    fn options(mode: &str, frames: u32, minimum: u32, maximum: u32) -> GenerationOptions {
        GenerationOptions {
            quality: "custom".into(),
            width: 64,
            height: 64,
            frames,
            fps: 12,
            frame_mode: mode.into(),
            min_frames: minimum,
            max_frames: maximum,
            allow_interpolation: false,
            allow_auto_adjust: mode == "auto",
        }
    }

    #[test]
    fn auto_mode_uses_motion_complexity_inside_user_limits() {
        let plan = build_motion_plan(
            "a heavy spinning greatsword combo",
            &options("auto", 6, 5, 14),
        )
        .expect("plan should build");
        assert_eq!(plan.selected_frame_count, 12);
        assert_eq!(
            plan.phases
                .iter()
                .map(|phase| phase.frame_count)
                .sum::<u32>(),
            12
        );
        assert!(plan.explanation.contains("multi-stage heavy action"));
    }

    #[test]
    fn fixed_mode_respects_the_exact_count_and_fits_phases() {
        let plan = build_motion_plan("quick sword slash", &options("fixed", 4, 4, 12))
            .expect("plan should build");
        assert_eq!(plan.selected_frame_count, 4);
        assert_eq!(plan.phases.len(), 4);
        assert_eq!(plan.minimum_frame_count, 4);
        assert_eq!(plan.maximum_frame_count, 4);
    }

    #[test]
    fn explicit_prompt_count_overrides_auto_mode() {
        let plan = build_motion_plan(
            "open the chest using exactly 10 frames",
            &options("auto", 6, 4, 8),
        )
        .expect("plan should build");
        assert_eq!(plan.frame_mode, "fixed");
        assert_eq!(plan.selected_frame_count, 10);
    }

    #[test]
    fn dynamic_motion_matrix_uses_the_smallest_readable_budget() {
        let cases = [
            ("idle breathing", 5, true),
            ("walk cycle", 8, true),
            ("run cycle", 8, true),
            ("sword attack", 7, false),
            ("heavy greatsword attack", 12, false),
            ("combat dodge", 6, false),
            ("spell cast", 9, false),
            ("character death", 10, false),
        ];
        for (prompt, expected_frames, expected_loop) in cases {
            let plan = build_motion_plan(prompt, &options("auto", 6, 4, 16))
                .unwrap_or_else(|error| panic!("{prompt} should plan: {}", error.message));
            assert_eq!(plan.selected_frame_count, expected_frames, "{prompt}");
            assert_eq!(plan.looping, expected_loop, "{prompt}");
            assert_eq!(
                plan.phases
                    .iter()
                    .map(|phase| phase.frame_count)
                    .sum::<u32>(),
                expected_frames,
                "{prompt}"
            );
        }
    }

    #[test]
    fn locomotion_phases_name_limbs_by_camera_depth_not_screen_side() {
        for prompt in ["run cycle", "walk cycle"] {
            let plan = build_motion_plan(prompt, &options("auto", 6, 4, 16))
                .unwrap_or_else(|error| panic!("{prompt} should plan: {}", error.message));
            let text = plan
                .phases
                .iter()
                .map(|phase| format!("{} {}", phase.name, phase.description))
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                text.contains("NEAR leg") && text.contains("FAR leg"),
                "{prompt} phases must name NEAR and FAR legs: {text}"
            );
            assert!(
                !text.to_lowercase().contains("left foot") && !text.to_lowercase().contains("right foot"),
                "{prompt} phases must not use screen-side leg names: {text}"
            );
            assert!(plan.explanation.contains("NEAR limb and FAR limb"));
        }
        // Non-locomotion motions keep their phase vocabulary untouched.
        let effect = build_motion_plan("explosion vfx loop", &options("auto", 6, 4, 16))
            .expect("effect plan should build");
        assert!(!effect.explanation.contains("NEAR limb"));
    }

    #[test]
    fn run_cycle_alternates_leg_contacts_and_keeps_flight_symmetric() {
        let plan = build_motion_plan("run cycle", &options("auto", 6, 4, 16))
            .expect("run plan should build");
        let names: Vec<&str> = plan.phases.iter().map(|phase| phase.name.as_str()).collect();
        let near_contacts = names.iter().filter(|name| **name == "Near contact").count();
        let far_contacts = names.iter().filter(|name| **name == "Far contact").count();
        assert_eq!(near_contacts, 1, "one NEAR contact per cycle: {names:?}");
        assert_eq!(far_contacts, 1, "one FAR contact per cycle: {names:?}");
        let flights = names
            .iter()
            .filter(|name| name.starts_with("Flight"))
            .count();
        assert_eq!(flights, 2, "a run cycle has two flight phases: {names:?}");
    }

    #[test]
    fn fixed_and_dynamic_sword_attacks_keep_distinct_contracts() {
        let fixed = build_motion_plan("sword attack", &options("fixed", 6, 4, 16))
            .expect("fixed plan should build");
        let dynamic = build_motion_plan("sword attack", &options("auto", 6, 4, 16))
            .expect("dynamic plan should build");
        assert_eq!(fixed.selected_frame_count, 6);
        assert_eq!(fixed.minimum_frame_count, 6);
        assert_eq!(fixed.maximum_frame_count, 6);
        assert!(!fixed.allow_auto_adjust);
        assert_eq!(dynamic.selected_frame_count, 7);
        assert_eq!(dynamic.minimum_frame_count, 4);
        assert_eq!(dynamic.maximum_frame_count, 16);
        assert!(dynamic.allow_auto_adjust);
    }
}
