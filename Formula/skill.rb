class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  require_relative "../custom_download_strategy"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.8/skill-darwin-arm64.tar.gz", using: GithubPrivateReleaseDownloadStrategy
  sha256 "1382a5ea8d6391b383b0e285a080914203256855321e49474f566b825ab238ad"
  version "0.0.8"

  def install
    bin.install "skill"
  end
end
