FROM ubuntu:26.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        clang \
        curl \
        git \
        lld \
        build-essential \
        pkg-config \
        procps \
        python3 \
        locales \
        strace \
        zsh \
        nodejs \
        npm \
    && rm -rf /var/lib/apt/lists/*

RUN locale-gen en_US.UTF-8 \
    && git clone --depth 1 https://github.com/ohmyzsh/ohmyzsh.git /opt/oh-my-zsh \
    && git clone --depth 1 https://github.com/zsh-users/zsh-autosuggestions.git /opt/zsh-autosuggestions \
    && git clone --depth 1 https://github.com/zsh-users/zsh-syntax-highlighting.git /opt/zsh-syntax-highlighting \
    && cat >/etc/zsh/zshrc <<'EOF'
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8
export TERM=xterm-256color
export CLICOLOR=1
export ZSH=/opt/oh-my-zsh

ZSH_THEME=""
plugins=(git colored-man-pages)

if [[ -r /opt/oh-my-zsh/oh-my-zsh.sh ]]; then
  source /opt/oh-my-zsh/oh-my-zsh.sh
fi

if [[ -r /opt/zsh-autosuggestions/zsh-autosuggestions.zsh ]]; then
  source /opt/zsh-autosuggestions/zsh-autosuggestions.zsh
fi

if [[ -r /opt/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh ]]; then
  source /opt/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
fi

autoload -Uz vcs_info
precmd() { vcs_info }
zstyle ':vcs_info:git:*' formats ' (%F{red}%b%f)'
zstyle ':vcs_info:*' enable git

setopt PROMPT_SUBST
PROMPT='%F{green}%n@${HOSTNAME}%f:%F{blue}%~%f${vcs_info_msg_0_}$ '

alias ls='ls --color=auto'
alias ll='ls -alF --color=auto'
alias grep='grep --color=auto'

HISTFILE=${HISTFILE:-/workspace/.cache/zsh_history}
HISTSIZE=10000
SAVEHIST=10000
setopt SHARE_HISTORY APPEND_HISTORY HIST_IGNORE_DUPS HIST_IGNORE_ALL_DUPS
setopt AUTO_CD EXTENDED_GLOB
EOF

RUN set -eu; \
    node_arch="$(dpkg --print-architecture)"; \
    case "${node_arch}" in \
      amd64) node_arch="x64" ;; \
      arm64) node_arch="arm64" ;; \
      *) echo "unsupported node architecture: ${node_arch}" >&2; exit 1 ;; \
    esac; \
    checksums_file="$(mktemp)"; \
    curl -fsSL "https://nodejs.org/dist/latest-v24.x/SHASUMS256.txt" -o "${checksums_file}"; \
    node_archive="$(awk -v node_arch="${node_arch}" '$2 ~ "^node-v24\\..*-linux-" node_arch "\\.tar\\.xz$" { print $2; exit }' "${checksums_file}")"; \
    test -n "${node_archive}"; \
    curl -fsSLO "https://nodejs.org/dist/latest-v24.x/${node_archive}"; \
    grep " ${node_archive}$" "${checksums_file}" | sha256sum -c -; \
    tar -xJf "${node_archive}" -C /usr/local --strip-components=1 --no-same-owner; \
    rm -f "${node_archive}" "${checksums_file}"; \
    node --version; \
    npm --version

SHELL ["/usr/bin/zsh", "-lc"]

ENV RUSTUP_HOME=/opt/rustup
ENV CARGO_HOME=/opt/cargo
ENV PATH=/opt/cargo/bin:${PATH}
ENV SHELL=/usr/bin/zsh

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable \
    && rustup update stable \
    && rustup toolchain install 1.96.0 --profile minimal \
    && rustup component add rustfmt clippy \
    && rustup component add rustfmt clippy --toolchain 1.96.0 \
    && chmod -R a+rwX /opt/rustup /opt/cargo

ENV CARGO_HOME=/workspace/.cache/cargo
ENV RUSTUP_HOME=/opt/rustup
ENV PATH=/opt/cargo/bin:${PATH}

WORKDIR /workspace

CMD ["/usr/bin/zsh"]
