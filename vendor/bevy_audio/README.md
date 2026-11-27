# Bevy Audio

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/bevyengine/bevy#license)
[![Crates.io](https://img.shields.io/crates/v/bevy_audio.svg)](https://crates.io/crates/bevy_audio)
[![Downloads](https://img.shields.io/crates/d/bevy_audio.svg)](https://crates.io/crates/bevy_audio)
[![Docs](https://docs.rs/bevy_audio/badge.svg)](https://docs.rs/bevy_audio/latest/bevy_audio/)
[![Discord](https://img.shields.io/discord/691052431525675048.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/bevy)

## Audio output processing

This copy uses Bevy revision `2e26874eb1a0f61ffd57d268be4e4725fe8cd4e2`
with its MIT and Apache licences. The optional `output_mix` module exposes
device-rate input mixers and processes their frames before writing to the
device. Stock players and spatial sinks route their output to those inputs, so
effects can operate after gain and panning and can finish their tails after a
voice ends. Without an installed processor, playback uses Bevy's default
device mixer.
