# Sprite quality gates

Before reporting completion, verify:

1. **Files:** every PNG exists under `assets/<category>/` and decodes successfully.
2. **Canvas:** every animation frame has identical logical dimensions.
3. **Transparency:** the background is transparent and corners have alpha zero.
4. **Silhouette:** the subject reads at 1× and does not touch unintended canvas edges.
5. **Pixel discipline:** no accidental antialiasing, blurry scaling, or isolated noise pixels.
6. **Palette:** outline, shadow, base, and highlight roles stay consistent across frames.
7. **Animation:** feet/pivot do not drift; motion arcs are intentional; looping frames do not pop unexpectedly.
8. **Originality:** style references are translated into general traits without copying a known character or exact sprite.
9. **Manifest:** `.sprite-studio/last-generation.json` lists files in playback order with the correct FPS and category.
10. **Clean handoff:** only the final manifest frames remain in `assets/`; the renderer archives superseded iterations under `.sprite-studio/versions/`.
11. **Master provenance:** character jobs and illustrated game objects have one approved master. Animation frames come from a saved rig under `.sprite-studio/rigs/`; ImageGen pose sheets and independently generated frames are rejected.
12. **Character consistency:** eye line, shoulders, head size, total height, face, hair, costume, palette, equipment, foot line, and pivot remain consistent across frames.
13. **Motion mechanics:** adjacent frames change intentional limb mechanics, walks contain opposing contact and passing poses, arms oppose legs, and the action reads during playback rather than only as still images.
14. **Rig reproducibility:** rerunning the same rig JSON against the same master produces identical frames, with connected joints/hinges and no exposed mask holes.

If a check fails, revise the spec and render again before responding.
