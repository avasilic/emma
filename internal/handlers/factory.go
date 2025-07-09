package handlers

import (
	"fmt"
)

func GetHandler(handlerType string) (Handler, error) {
	switch handlerType {
	case "http_fetch":
		return NewHTTPFetcher(), nil
	case "web_scraper":
		// TODO: Implement web scraper
		return nil, fmt.Errorf("web_scraper not implemented yet")
	case "ftp_download":
		// TODO: Implement FTP downloader
		return nil, fmt.Errorf("ftp_download not implemented yet")
	default:
		return nil, fmt.Errorf("unknown handler type: %s", handlerType)
	}
}
