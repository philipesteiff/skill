class Skill < Formula
  desc "CLI for managing skills"
  homepage "https://github.com/philipesteiff/skills"
  url "https://github.com/philipesteiff/skills/releases/download/v0.0.1/skill-darwin-arm64.tar.gz"
  sha256 "5fc728260f014cfc8a7950a673a4bcec1671db687b50fc0f255d8dbdecb92311"
  version "0.0.1"

  def install
    bin.install "skill"
  end
end
