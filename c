#!/usr/bin/env bash

# Inspired by https://github.com/rti/nixwrap/blob/main/wrap.sh


positional_args=()

while [[ $# -gt 0 ]]; do
  case $1 in
    # -e|--extension)
    #   EXTENSION="$2"
    #   shift # past argument
    #   shift # past value
    #   ;;
    --net)
      net=YES
      shift # past argument
      ;;
    -*|--*)
      echo "Unknown option $1"
      exit 1
      ;;
    *)
      positional_args+=("$1") # save positional arg
      shift # past argument
      ;;
  esac
done

set -- "${positional_args[@]}" # restore positional parameters


if [ "$#" -ne 1 ]; then
    echo "Missing name argument"
    exit 1
fi

name=$1

# environment variables always shared with the wrapped process
# for usability and convenience
env_vars=(
  EDITOR
  HOME
  INFOPATH
  LD_LIBRARY_PATH
  LIBEXEC_PATH
  PAGER
  PATH
  PKG_CONFIG_PATH
  PWD
  SHELL

  TERM
  TERMINFO
  TERMINFO_DIRS

  XDG_BIN_HOME
  XDG_CACHE_HOME
  XDG_CONFIG_DIRS
  XDG_CONFIG_HOME
  XDG_CURRENT_DESKTOP
  XDG_DATA_DIRS
  XDG_DATA_HOME
  XDG_DESKTOP_PORTAL_DIR
  XDG_RUNTIME_DIR
  XDG_SEAT
  XDG_SESSION_CLASS
  XDG_SESSION_ID
  XDG_SESSION_TYPE
  XDG_VTNR

  NIXPKGS_ALLOW_UNFREE
  NIXPKGS_CONFIG
  NIX_PATH
  NIX_PROFILES
  NIX_USER_PROFILE_DIR

  LANG
  LC_ADDRESS
  LC_IDENTIFICATION
  LC_MEASUREMENT
  LC_MONETARY
  LC_NAME
  LC_NUMERIC
  LC_PAPER
  LC_TELEPHONE
  LC_TIME
  LOCALE_ARCHIVE
  LOCALE_ARCHIVE_2_27
  TZDIR
)

# environment variables only shared with the wrapped process
# when running with -d desktop access
env_vars_desktop=(
  DISPLAY
  WAYLAND_DISPLAY

  DESKTOP_STARTUP_ID

  BROWSER

  GDK_BACKEND
  GTK2_RC_FILES
  GTK_A11Y
  GTK_PATH

  MOZ_ENABLE_WAYLAND
  NIXOS_OZONE_WL
  QT_QPA_PLATFORM
  QT_WAYLAND_DISABLE_WINDOWDECORATION
  QT_WAYLAND_FORCE_DPI
  SDL_VIDEODRIVER
  VDPAU_DRIVER

  WLR_LIBINPUT_NO_DEVICES

  XCURSOR_PATH
  XCURSOR_SIZE
  XCURSOR_THEME
)

# paths shared read only by default
paths_general=(
  /nix
  /etc/nix
  /etc/static/nix

  /bin
  /usr
  /lib
  /lib64
)

bwrap_opts=()

for p in "${paths_general[@]}"; do
  if [ -d "$p" ]; then
    bwrap_opts+=(--ro-bind "$p" "$p")
  fi
done

# Append all desktop-related env variables
env_vars+=("${env_vars_desktop[@]}")

for e in "${env_vars[@]}"; do
  if [ -v "$e" ]; then
    bwrap_opts+=(--setenv "$e" "${!e}")
  fi
done

# grant desktop access (Wayland or X11) and rendering hardware access
if [ -n "${WAYLAND_DISPLAY:-}" ] && [ -n "${XDG_RUNTIME_DIR:-}" ]; then
  # Using Wayland: bind the Wayland display socket
  bwrap_opts+=(--bind "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY")
fi

if [ -n "${DISPLAY:-}" ]; then
  # Using X11: bind the X11 socket directory
  # The standard location is usually /tmp/.X11-unix.
  if [ -d "/tmp/.X11-unix" ]; then
    bwrap_opts+=(--ro-bind "/tmp/.X11-unix" "/tmp/.X11-unix")
  fi

  # Bind the .Xauthority file so that the authorization data is available.
  if [ -n "${XAUTHORITY:-}" ]; then
    # Bind a custom path Xauthority file to the standard path in the sandbox
    bwrap_opts+=(--ro-bind "${XAUTHORITY}" "$HOME/.Xauthority")
  elif [ -f "$HOME/.Xauthority" ]; then
    # Bind the standard path Xauthority file to the sandbox
    bwrap_opts+=(--ro-bind "$HOME/.Xauthority" "$HOME/.Xauthority")
  fi
fi

if [ -d /dev/dri ]; then
  bwrap_opts+=(--dev-bind /dev/dri /dev/dri)
fi

if [ -d /run/opengl-driver ]; then
  bwrap_opts+=(--ro-bind /run/opengl-driver /run/opengl-driver)
fi

if [ -d /sys ]; then
  bwrap_opts+=(--ro-bind /sys/ /sys/)
fi

if [ -d /etc/fonts ]; then
  bwrap_opts+=(--ro-bind /etc/fonts /etc/fonts)
fi

bwrap_opts+=(--ro-bind /etc/alternatives /etc/alternatives)
bwrap_opts+=(--ro-bind /etc/man_db.conf /etc/man_db.conf)
bwrap_opts+=(--ro-bind /etc/localtime /etc/localtime)
bwrap_opts+=(--ro-bind /etc/passwd /etc/passwd)
bwrap_opts+=(--ro-bind /etc/crypto-policies/ /etc/crypto-policies/)

bwrap_opts+=(--unshare-all)
if [[ $net == YES ]]; then
    bwrap_opts+=(--share-net)
    # systemd-resolve for dns
    # bwrap_opts+=(--ro-bind /run/systemd/resolve /run/systemd/resolve)
    bwrap_opts+=(--ro-bind /etc/resolv.conf /etc/resolv.conf)
    bwrap_opts+=(--ro-bind /etc/ssl /etc/ssl)
    bwrap_opts+=(--ro-bind /etc/pki /etc/pki)
fi

mkdir -p $HOME/homes/$name
bwrap_opts+=(--bind $HOME/homes/$name $HOME)
bwrap_opts+=(--hostname $name)

exec bwrap \
    --new-session \
    --clearenv \
    --dev /dev \
    --proc /proc \
    --tmpfs /tmp \
    --dir "$HOME" \
    "${bwrap_opts[@]}" \
    /usr/bin/ghostty
