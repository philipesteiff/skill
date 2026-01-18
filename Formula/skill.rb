class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  require_relative "../custom_download_strategy"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.6/skill-darwin-arm64.tar.gz", using: GithubPrivateReleaseDownloadStrategy
  sha256 "efa6d9103f7f726d981d84d27b647a0c3e76c7f54989b325a5aaa193924f2044"
  version "0.0.6"

  def install
    bin.install "skill"
  end
end
