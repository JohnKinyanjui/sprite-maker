# Sprite rig profiles

Every new articulated animation uses `rigVersion: 3` and exactly one anatomy-specific `rigProfile`. Do not use one generic mask recipe for every subject.

## Shared order of work

1. Inspect the source at high zoom and mark every observed joint before drawing masks.
2. Write the bind/rest skeleton and `joints` evidence. Each entry records `name`, anatomical `kind`, source-space `position`, `visibility`, and the two parts meeting there.
3. Reject the source as motion-unready when a required gameplay-side joint is merged, hidden, or invented. In an authorized polish mode, create one motion-ready source revision and reinspect it.
4. Segment upper, lower, and end-effector parts around the observed joints. Give every part a `bone` capsule with `startAnchor`, `endAnchor`, and `radius`.
5. Verify the base contains no opaque pixels inside any bone capsule. A leftover pixel is not a repair patch; it means the anatomical mask is incomplete.
6. Build named key poses before filling in breakdowns or in-betweens. Mesh weighting may soften a proven chain, but may never substitute for a missing joint.

## `human_sprite_rig`

Use for morphology `biped`. Locomotion requires observed left/right hip, knee, and ankle joints, separate thigh/shin/foot chains, a pelvis/torso chain, and upper-body opposition when visible. Walk poses include left/right `contact`, `down`, `passing`, and `up`. Runs add compression, airborne passing, and recovery. IK owns planted feet; pelvis bob and torso counter-rotation follow the support leg.

## `four_leg_sprite_rig`

Use for morphology `quadruped`. Create near/far fore and hind chains with at least three pieces each. Hind chains expose hip, knee/stifle, and ankle/hock; fore chains expose shoulder, elbow, and wrist/carpus. The six gameplay-side joints must be visibly supported by source pixels. A run/gallop requires named `hind_contact`, `extended_flight`, `fore_contact`, and `gathered_flight` poses. The spine compresses between pelvis and chest, the head stabilizes, and the tail follows with delay. A standing silhouette warped into different shapes is not a quadruped gait.

## `multi_leg_sprite_rig`

Use for `hexapod` and `segmented-many-leg`. Name body-segment joints and each visible leg bank. Hexapods use alternating tripod poses; many-leg creatures use a metachronal wave. Do not merge every leg on one side into a blinking texture.

## `serpentine_sprite_rig`

Use for `serpentine`. Build a head-to-tail chain with visible bend stations and phase-shifted poses. Preserve arc length and continue the body wave through the loop seam.

## `winged_sprite_rig`

Use for `winged`. Separate shoulder, elbow/carpal, and wing-tip joints from grounded leg chains. Key poses distinguish downstroke, reversal, upstroke, and recovery; takeoff and landing also name planted contacts.

## Version 3 JSON additions

```json
{
  "rigVersion": 3,
  "rigProfile": "four_leg_sprite_rig",
  "joints": [
    {
      "name": "near_hind_knee",
      "kind": "knee",
      "position": [22, 46],
      "visibility": "visible",
      "parts": ["near_hind_upper", "near_hind_lower"]
    }
  ],
  "parts": [
    {
      "name": "near_hind_upper",
      "bone": {"startAnchor": "hip", "endAnchor": "knee", "radius": 3}
    }
  ],
  "frames": [
    {"phase": "hind drive", "pose": "hind_contact"}
  ]
}
```

The renderer validates this evidence against the actual source alpha. Merely naming a joint does not pass: a visible joint must lie next to pixels owned by both connected parts.
