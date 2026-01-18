class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.10/skill-darwin-arm64.tar.gz"
  sha256 "8e2654c22a7c8c65c47b27c0bdff48db342ea10098c1b13172863376d0e43fce"
  version "0.0.10"

  def install
    bin.install "skill"
  end
end
