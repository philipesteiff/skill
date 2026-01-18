class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  require_relative "../custom_download_strategy"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.5/skill-darwin-arm64.tar.gz", using: GithubPrivateReleaseDownloadStrategy
  sha256 "c604ee62d1cf80aa58b6a0dd1cc2b7b4f73d8a6da48b045322653750c38bad14"
  version "0.0.5"

  def install
    bin.install "skill"
  end
end
