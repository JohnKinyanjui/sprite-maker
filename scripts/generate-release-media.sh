#!/usr/bin/env bash
set -euo pipefail

workspace="${1:-/private/tmp/sprite-studio-e2e.XFf6J7}"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
output="$repo_root/docs/media"
scratch="$(mktemp -d /private/tmp/sprite-studio-release-media.XXXXXX)"
trap 'rm -rf "$scratch"' EXIT

mkdir -p "$output"

make_stage() {
  local title="$1"
  local subtitle="$2"
  local badge="$3"
  local destination="$4"

  magick -size 960x540 xc:'#0b0b0c' \
    -fill '#171719' -stroke '#2b2b2e' -strokewidth 1 \
    -draw 'roundrectangle 72,90 888,458 18,18' \
    -fill '#f4f4f5' -stroke none -font Arial-Bold -pointsize 30 \
    -draw "text 72,48 '$title'" \
    -fill '#929299' -font Arial -pointsize 16 \
    -draw "text 72,73 '$subtitle'" \
    -fill '#232326' -stroke '#39393d' \
    -draw 'roundrectangle 734,25 888,62 18,18' \
    -fill '#d6d6d9' -stroke none -font Arial-Bold -pointsize 14 \
    -gravity north -annotate +330+36 "$badge" \
    -gravity southwest -fill '#77777f' -font Arial -pointsize 14 \
    -annotate +72+32 'Generated, rigged, tested, and exported in Sprite Studio' \
    "$destination"
}

render_animation() {
  local title="$1"
  local subtitle="$2"
  local badge="$3"
  local delay="$4"
  local geometry="$5"
  local output_name="$6"
  shift 6
  local sources=("$@")
  local base="$scratch/${output_name%.gif}-base.png"
  local frames_dir="$scratch/${output_name%.gif}-frames"
  mkdir -p "$frames_dir"
  make_stage "$title" "$subtitle" "$badge" "$base"

  local index=0
  for source in "${sources[@]}"; do
    index=$((index + 1))
    magick "$base" \
      \( "$source" -filter point -resize "$geometry" \) \
      -gravity center -geometry +0+8 -composite \
      "$frames_dir/$(printf '%03d' "$index").png"
  done

  magick -delay "$delay" -loop 0 "$frames_dir"/*.png \
    -layers OptimizeTransparency -colors 256 "$output/$output_name"
}

rabbit=("$workspace"/assets/props/forest_rabbit_hop_forward_polish_v1_*.png)
centipede=("$workspace"/assets/creatures/cave_centipede_crawl_*.png)
dragon=("$workspace"/assets/props/cozy_chibi_dragon_flight_*.png)
pack=(
  "$workspace/assets/props/grasslands-round-oak.png"
  "$workspace/assets/props/grasslands-slender-birch.png"
  "$workspace/assets/props/grasslands-flowering-tree.png"
  "$workspace/assets/props/grasslands-evergreen-pine.png"
  "$workspace/assets/props/grasslands-leafy-bush.png"
  "$workspace/assets/props/grasslands-berry-bush.png"
  "$workspace/assets/props/grasslands-mossy-rocks.png"
  "$workspace/assets/props/grasslands-wildflowers.png"
)

render_animation \
  'A hop with weight, not a vertical slide' \
  'Eight planned poses: load, launch, apex, reach, contact, and recovery' \
  'RABBIT · 8 FPS' 12 '320x320' 'rabbit-hop.gif' "${rabbit[@]}"

render_animation \
  'Segmented motion stays connected' \
  'Twelve phase-shifted body and leg states close into one crawl loop' \
  'CENTIPEDE · 12 FPS' 8 '448x448' 'centipede-crawl.gif' "${centipede[@]}"

render_animation \
  'One dragon, one continuous wingbeat' \
  'Twelve rigged frames preserve the source design through downstroke and recovery' \
  'DRAGON · 8 FPS' 12 '320x320' 'dragon-flight.gif' "${dragon[@]}"

pack_base="$scratch/pack-base.png"
pack_frames="$scratch/pack-frames"
mkdir -p "$pack_frames"
make_stage \
  'Build a coordinated art pack in one prompt' \
  'Every asset shares the same palette, outline, scale language, and production style' \
  'PACK · 8 ASSETS' "$pack_base"

positions=("+170+190" "+375+190" "+580+190" "+785+190" "+170+360" "+375+360" "+580+360" "+785+360")
for active in "${!pack[@]}"; do
  frame="$pack_base"
  current="$scratch/pack-$(printf '%02d' "$active").png"
  cp "$frame" "$current"
  for item in "${!pack[@]}"; do
    x="${positions[$item]%%+*}"
    remainder="${positions[$item]#*+}"
    x="${remainder%%+*}"
    y="${remainder##*+}"
    if [[ "$item" == "$active" ]]; then
      magick "$current" -fill '#f4f4f510' -stroke '#f4f4f5' -strokewidth 2 \
        -draw "roundrectangle $((x-76)),$((y-76)) $((x+76)),$((y+76)) 12,12" "$current"
    fi
    magick "$current" \
      \( "${pack[$item]}" -filter point -resize '136x136' \) \
      -gravity northwest -geometry "+$((x-68))+$((y-68))" -composite "$current"
  done
  cp "$current" "$pack_frames/$(printf '%03d' "$((active+1))").png"
done
magick -delay 35 -loop 0 "$pack_frames"/*.png \
  -layers OptimizeTransparency -colors 256 "$output/grasslands-pack.gif"

cp "$workspace/assets/terrain/beautiful_grasslands_ponds_tileset.png" \
  "$output/grasslands-terrain.png"

# A compact single-GIF release reel for social posts. Each animation remains
# pixel-perfect because the already-rendered showcase frames are only resized
# with point sampling before being concatenated.
social_frames="$scratch/social-frames"
mkdir -p "$social_frames"
counter=0
for sequence in \
  "$scratch/rabbit-hop-frames" \
  "$scratch/dragon-flight-frames" \
  "$scratch/centipede-crawl-frames" \
  "$pack_frames"; do
  for source in "$sequence"/*.png; do
    counter=$((counter + 1))
    magick "$source" -filter point -resize 800x450! \
      "$social_frames/$(printf '%03d' "$counter").png"
  done
done
magick -delay 10 -dispose background -loop 0 "$social_frames"/*.png \
  -layers Optimize -colors 192 "$output/sprite-studio-v0.2-showcase.gif"

magick identify "$output/rabbit-hop.gif" "$output/centipede-crawl.gif" \
  "$output/dragon-flight.gif" "$output/grasslands-pack.gif" \
  "$output/sprite-studio-v0.2-showcase.gif" "$output/grasslands-terrain.png"
