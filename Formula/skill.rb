class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  require_relative "../custom_download_strategy"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.7/skill-darwin-arm64.tar.gz", using: GithubPrivateReleaseDownloadStrategy
  sha256 "322c0964699b9f05544ae033e5bef8b0bc3144f3744960b89c6735d81f194bd8"
  version "0.0.7"

  def install
    bin.install "skill"
  end
end
