#!/bin/sh
# Puts the RigHelm intro in front of the band scope video, in place of the
# band scope video's own title card.
#
#   ./with_intro.sh <intro.mp4> [bandscope.mp4]   -> righelm-bandscope-full.mp4
set -e
cd "$(dirname "$0")"
INTRO=${1:?usage: with_intro.sh <intro.mp4> [bandscope.mp4]}
MAIN=${2:-righelm-bandscope.mp4}
# The band scope video's title card is gone by 4.4 s.
CUT=4.4
ffmpeg -y -loglevel error -i "$INTRO" -ss $CUT -i "$MAIN" -filter_complex "
  [0:v]scale=1920:1080:force_original_aspect_ratio=decrease:flags=lanczos,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=30,format=yuv420p,setsar=1[v0];
  [1:v]fps=30,format=yuv420p,setsar=1,fade=t=in:st=0:d=0.5[v1];
  [0:a]aresample=48000,aformat=channel_layouts=stereo[a0];
  [1:a]aresample=48000,aformat=channel_layouts=stereo,afade=t=in:st=0:d=0.8[a1];
  [v0][a0][v1][a1]concat=n=2:v=1:a=1[v][a]" \
  -map "[v]" -map "[a]" -c:v libx264 -preset slow -crf 17 -pix_fmt yuv420p -c:a aac -b:a 192k -movflags +faststart \
  righelm-bandscope-full.mp4
echo "video -> $PWD/righelm-bandscope-full.mp4"
