class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.4/skill-darwin-arm64.tar.gz"
  sha256 "def2f294966a90d1862e1745ee16f62d1e0f0d085ce5d17e65e9e2891b3f0b2f"
  version "0.0.4"

  def install
    bin.install "skill"
  end
end
