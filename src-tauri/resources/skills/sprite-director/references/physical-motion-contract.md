# Real-world motion and scale contract

Use this contract for every character, creature, prop, vehicle, plant, or environmental animation. Real-world mechanics are the default unless the user explicitly supplies a different speed, distance, height, duration, gravity, scale, or asks for exaggerated/magical/cartoon motion.

## Establish a physical envelope

After visually inspecting the source and identifying the motion verb, state:

`PHYSICAL ENVELOPE: scale <meters>; speed <m/s>; vertical rise <meters>; cycle <seconds>; gravity/contact <assumption> — <reason>`

- Estimate the subject's plausible real-world height, length, or diameter as a range, then pick one nominal value for conversion. For a fantasy creature, use the closest visible real-world analogue and say that it is an estimate.
- Use plausible locomotion speeds, jump/hop heights, stride lengths, angular velocities, fall timing, material response, and acceleration for that subject and action. Distinguish an ordinary gait from a sprint, panic movement, attack burst, or heavy mechanical cycle.
- Do not make a rabbit, human, centipede, tree, door, vehicle, or heavy object share the same generic pixel arc or timing. Body size, mass, limb length, support pattern, material, and environment change the result.
- Grounded motion obeys contacts and weight transfer. Airborne motion has takeoff, ballistic rise, apex, fall, landing impact, and recovery. Hinges and machines respect their pivots, inertia, stops, and damping. Wind-driven plants bend from their anchored base and lag progressively toward lighter tips.
- User-stated physical quantities and explicit stylization override estimates. Preserve the user's values and report them; do not silently clamp a requested ten-meter magical leap back to realism.

## Convert meters and seconds to sprite motion

Never assume one pixel equals one meter. Derive a temporary scene scale from the observed subject:

`pixels_per_meter = observed_subject_pixel_height_or_length / nominal_subject_height_or_length_meters`

Use that scale to calculate the first physical motion proposal. Then convert it to a readable in-place sprite cycle:

- report intended world displacement in meters and the equivalent body lengths per cycle;
- report peak vertical rise in meters, body-height fractions, and the corresponding unclipped pixel arc;
- use FPS and cycle duration to derive frame timing; Auto mode may recommend more frames when the physically required phases cannot read at the current FPS;
- keep locomotion animation spatially stable for game use and store/report world velocity separately unless the user explicitly requests baked root travel;
- if the logical canvas cannot contain the physically scaled arc, enlarge the canvas when allowed or preserve the body-length proportion and report the display-scale adjustment. Never flatten the jump merely to avoid clipping;
- treat sprite pixels as visual sampling of continuous motion: round transforms carefully while preserving contacts, phase order, and the final-to-first transition.

## Sanity checks

Before rendering, reject and revise a proposal when:

1. speed × cycle duration does not approximately match reported world displacement;
2. airborne time, peak height, and gravity assumption contradict one another without an explicit stylized override;
3. stride or jump distance is implausible for the observed limb/body proportions;
4. a planted foot, wheel contact, hinge, or anchored base violates the physical envelope;
5. the animation's seconds-per-cycle do not match frame count ÷ FPS;
6. the final-to-first transition implies a teleport, momentum reversal, or impossible contact change.

Include the chosen physical values and any user override in the animation provenance report and summarize them in the final response. These estimates guide motion; they are not claims of exact biological measurement.
