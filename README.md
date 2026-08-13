# git-ebb

`git branch`, enriched with GitHub PR titles.

## Install

```sh
cargo install git-ebb
```

## Usage

Designed for use in combination with [skim](https://github.com/skim-rs/skim),
ie:

```sh
sk --ansi -c git-ebb | cut -f1 -w
```
