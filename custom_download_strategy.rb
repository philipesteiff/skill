require "download_strategy"

class GithubPrivateReleaseDownloadStrategy < CurlDownloadStrategy
  def initialize(url, name, version, **meta)
    super
    parse_api_url
  end

  private

  def parse_api_url
    url_match = @url.match(%r{https://github\.com/([^/]+)/([^/]+)/releases/download/([^/]+)/(.+)})
    raise "Invalid URL: #{@url}" unless url_match

    owner = url_match[1]
    repo = url_match[2]
    tag = url_match[3]
    filename = url_match[4]

    @api_url = "https://api.github.com/repos/#{owner}/#{repo}/releases/tags/#{tag}"
    @filename = filename
  end

  def api_headers
    token = ENV["HOMEBREW_GITHUB_API_TOKEN"] || ENV["HOMEBREW_GITHUB_TOKEN"]
    return {} if token.nil? || token.empty?

    {"Authorization" => "token #{token}"}
  end

  def fetch_release
    curl_download(@api_url, temporary_path, *api_headers_to_args, "-H", "Accept: application/vnd.github+json")
    JSON.parse(File.read(temporary_path))
  end

  def api_headers_to_args
    api_headers.flat_map { |k, v| ["-H", "#{k}: #{v}"] }
  end

  def _fetch(url:, **)
    json = fetch_release
    assets = json.fetch("assets", [])
    asset = assets.find { |entry| entry["name"] == @filename }
    raise "Asset #{@filename} not found in #{@api_url}" if asset.nil?

    download_url = asset.fetch("url")
    curl_download(download_url, cached_location, *api_headers_to_args, "-H", "Accept: application/octet-stream")
  end
end
