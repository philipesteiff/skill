class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.9/skill-darwin-arm64.tar.gz"
  sha256 "92ca40184b69a9b13bbc4dfee4aec9183964b43349a658a0b7a915714e7fe7a6"
  version "0.0.9"

  def install
    bin.install "skill"
  end
end
