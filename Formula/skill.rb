class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skill"
  url "https://github.com/philipesteiff/skill/releases/download/v0.0.2/skill-darwin-arm64.tar.gz"
  sha256 "75926c26e011a3fb6176e1cd26ab9c92463b860b0b1f11d1161791689c9068cc"
  version "0.0.2"

  def install
    bin.install "skill"
  end
end
