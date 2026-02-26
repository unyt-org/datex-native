#!/usr/bin/env bash
# Copyright (C) 2025 unyt.org <webmaster@unyt.org>
# All rights reserved. MIT license.
# Usage: install.sh <optional tag>
set -eo pipefail

platform=$(uname -ms)

if [ "$OS" = "Windows_NT" ]; then
	if [[ $platform != MINGW64* ]]; then
		powershell -c "irm https://raw.githubusercontent.com/unyt-org/datex-native/main/install.ps1|iex"
		exit $?
	fi
fi

# Defaults
tildify() {
	if [[ $1 = $HOME/* ]]; then
		local replacement=\~/

		echo "${1/$HOME\//$replacement}"
	else
		echo "$1"
	fi
}

# ENV setup
GITHUB="https://github.com/unyt-org/datex-native"
GITHUB_API="https://api.github.com/repos/unyt-org/datex-native"

# Logging
Color_Off=''
Red=''
Green=''
Dim=''
Bold_White=''
Bold_Green=''

if [[ -t 1 ]]; then
	Color_Off='\033[0m'
	Red='\033[0;31m'
	Green='\033[0;32m'
	Dim='\033[0;2m'
	Bold_Green='\033[1;32m'
	Bold_White='\033[1m'
fi

error() {
	echo -e "${Red}error${Color_Off}:" "$@" >&2
	exit 1
}
info() {
	echo -e "${Dim}$@ ${Color_Off}"
}
info_bold() {
	echo -e "${Bold_White}$@ ${Color_Off}"
}
success() {
	echo -e "${Green}$@ ${Color_Off}"
}

# Validate args
if [[ $# -gt 1 ]]; then
	error 'Too many arguments passed. You can only pass a specific tag of DATEX to be installed. (e.g. "v0.1.4")'
fi

# Check for zip utility
if ! command -v unzip >/dev/null && ! command -v 7z >/dev/null; then
	error "Either unzip or 7z is required to install DATEX." 1>&2
	exit 1
fi

# Detect target
case $platform in
	"Darwin x86_64") target="x86_64-apple-darwin" ;;
	"Darwin arm64") target="aarch64-apple-darwin" ;;
	"Linux aarch64") target="aarch64-unknown-linux-gnu" ;;
	'MINGW64'*) target="x86_64-pc-windows-msvc" ;;
	*) target="x86_64-unknown-linux-gnu" ;;
esac

if [[ $target = "darwin-x64" ]]; then
	# Is this process running in Rosetta?
	# redirect stderr to devnull to avoid error message when not running in Rosetta
	if [[ $(sysctl -n sysctl.proc_translated 2>/dev/null) = 1 ]]; then
		target=darwin-aarch64
		info "Your shell is running in Rosetta 2. Downloading DATEX for $target instead."
	fi
fi

datex_version=''
if [[ $# -eq 0 ]]; then
  datex_version=$(curl -sL "$GITHUB_API/releases/latest" | jq -r '.tag_name')
else
  tag=$1
  release_uri="$GITHUB_API/releases/tags/$tag"
  http_code=$(curl -sL -o /dev/null -w '%{http_code}' "$release_uri")
  if [[ "$http_code" != 200 ]]; then
    echo "❌  Tag '$tag' not found at $GITHUB_API/releases" >&2
    exit 1
  fi
  datex_version=$tag
fi
# if datex_version "null" or empty, error out
if [[ -z $datex_version || $datex_version == "null" ]]; then
	error "Failed to determine DATEX version. Please specify a valid tag."
	exit 1
fi

info_bold "Installing DATEX version $datex_version"

# prepare installation directory
datex_install_dir="${DATEX_INSTALL:-$HOME/.datex}"
bin_dir="$datex_install_dir/bin"
exe="$bin_dir/datex"
if [ ! -d "$bin_dir" ]; then
	mkdir -p "$bin_dir"
fi

# download executable
artifact_uri="${GITHUB}/releases/download/${datex_version}/datex-${target}.zip"

rm -f "$exe.zip"
curl --fail --location --progress-bar --output "$exe.zip" "$artifact_uri" ||
	error "Failed to download DATEX from \"$artifact_uri\""

# unzip executable
if command -v unzip >/dev/null; then
	unzip -oqd "$bin_dir" -o "$exe.zip"
else
	7z x -o"$bin_dir" -y "$exe.zip"
fi
rm "$exe.zip"

# give permissions
if [ -e "$exe" ]; then
	chmod +x "$exe" ||
		error 'Failed to set permissions on DATEX executable.'
else
	error "DATEX executable not found at $exe"
fi

success "DATEX was installed successfully to $Bold_Green$(tildify "$exe")!"

# shell detection for persistent installation
refresh_command=''

tilde_bin_dir=$(tildify "$bin_dir")
quoted_install_dir=\"${datex_install_dir//\"/\\\"}\"
if [[ $quoted_install_dir = \"$HOME/* ]]; then
	quoted_install_dir=${quoted_install_dir/$HOME\//\$HOME/}
fi

install_env=DATEX_INSTALL
bin_env=\$$install_env/bin

case $(basename "$SHELL") in
fish)
	# Install completions, but we don't care if it fails
	SHELL=fish $exe completions &>/dev/null || :
	commands=(
		"set --export $install_env $quoted_install_dir"
		"set --export PATH $bin_env \$PATH"
	)

	fish_config=$HOME/.config/fish/config.fish
	tilde_fish_config=$(tildify "$fish_config")
	if [[ -w $fish_config ]]; then
		{
			echo -e "\n# DATEX"

			for command in "${commands[@]}"; do
				echo "$command"
			done
		} >>"$fish_config"
		info "Added \"$tilde_bin_dir\" to \$PATH in \"$tilde_fish_config\""
		refresh_command="source $tilde_fish_config"
	else
		echo "Manually add the directory to $tilde_fish_config (or similar):"
		for command in "${commands[@]}"; do
			info_bold "  $command"
		done
	fi
	;;
	zsh)
	# Install completions, but we don't care if it fails
	SHELL=zsh $exe completions &>/dev/null || :

	commands=(
		"export $install_env=$quoted_install_dir"
		"export PATH=\"$bin_env:\$PATH\""
	)

	zsh_config=$HOME/.zshrc
	tilde_zsh_config=$(tildify "$zsh_config")

	if [[ -w $zsh_config ]]; then
		{
			echo -e "\n# DATEX"
			for command in "${commands[@]}"; do
				echo "$command"
			done
		} >>"$zsh_config"

		info "Added \"$tilde_bin_dir\" to \$PATH in \"$tilde_zsh_config\""
		refresh_command="exec $SHELL"
	else
		echo "Manually add the directory to $tilde_zsh_config (or similar):"
		for command in "${commands[@]}"; do
			info_bold "  $command"
		done
	fi
	;;
	bash)
	# Install completions, but we don't care if it fails
	SHELL=bash $exe completions &>/dev/null || :
	commands=(
		"export $install_env=$quoted_install_dir"
		"export PATH=\"$bin_env:\$PATH\""
	)
	bash_configs=(
		"$HOME/.bashrc"
		"$HOME/.bash_profile"
	)

	if [[ ${XDG_CONFIG_HOME:-} ]]; then
		bash_configs+=(
			"$XDG_CONFIG_HOME/.bash_profile"
			"$XDG_CONFIG_HOME/.bashrc"
			"$XDG_CONFIG_HOME/bash_profile"
			"$XDG_CONFIG_HOME/bashrc"
		)
	fi

	set_manually=true
	for bash_config in "${bash_configs[@]}"; do
		tilde_bash_config=$(tildify "$bash_config")
		if [[ -w $bash_config ]]; then
			{
				echo -e "\n# DATEX"
				for command in "${commands[@]}"; do
					echo "$command"
				done
			} >>"$bash_config"

			info "Added \"$tilde_bin_dir\" to \$PATH in \"$tilde_bash_config\""
			refresh_command="source $bash_config"
			set_manually=false
			break
		fi
	done

	if [[ $set_manually = true ]]; then
		echo "Manually add the directory to $tilde_bash_config (or similar):"
		for command in "${commands[@]}"; do
			info_bold "  $command"
		done
	fi
	;;
*)
	echo 'Manually add the directory to ~/.bashrc (or similar):'
	info_bold "  export $install_env=$quoted_install_dir"
	info_bold "  export PATH=\"$bin_env:\$PATH\""
	;;
esac

echo
info "To get started with DATEX, run:"
echo

if [[ $refresh_command ]]; then
	info_bold "  $refresh_command"
fi

info_bold "  datex"