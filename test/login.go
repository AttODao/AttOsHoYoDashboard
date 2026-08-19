// login.go is a read-only integration check for the unofficial HoYoLAB API.
//
// Run from the repository root after creating test/.env:
//
//	cp test/.env.example test/.env
//	go run ./test/login.go
package main

import (
	"bufio"
	"bytes"
	"context"
	"crypto/md5" // #nosec G501 -- MD5 is required by HoYoLAB's DS protocol, not used for security here.
	"crypto/rand"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	defaultEnvPath = "test/.env"
	defaultLang    = "ja-jp"
	defaultUA      = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.36"
	overseasDSSalt = "6s25p5ox5y14umn1p61aqyyvbvvl3lrt"
	maxResponse    = 4 << 20
)

type apiEnvelope struct {
	Retcode int             `json:"retcode"`
	Message string          `json:"message"`
	Data    json.RawMessage `json:"data"`
}

type gameAccountsData struct {
	List []gameAccount `json:"list"`
}

type gameAccount struct {
	GameBiz    string `json:"game_biz"`
	Region     string `json:"region"`
	GameUID    string `json:"game_uid"`
	Nickname   string `json:"nickname"`
	Level      int    `json:"level"`
	RegionName string `json:"region_name"`
}

type dailyInfo struct {
	TotalSignDay int  `json:"total_sign_day"`
	IsSign       bool `json:"is_sign"`
	FirstBind    bool `json:"first_bind"`
}

type gameSpec struct {
	Name           string
	GameBiz        string
	SignGame       string
	DailyInfoURL   string
	RecordBaseURL  string
	RecordEndpoint string
	NotesEndpoint  string
}

var games = []gameSpec{
	{
		Name:           "原神",
		GameBiz:        "hk4e_global",
		SignGame:       "hk4e",
		DailyInfoURL:   "https://sg-hk4e-api.hoyolab.com/event/sol/info?act_id=e202102251931481",
		RecordBaseURL:  "https://sg-public-api.hoyolab.com/event/game_record/genshin/api",
		RecordEndpoint: "index",
		NotesEndpoint:  "dailyNote",
	},
	{
		Name:           "崩壊：スターレイル",
		GameBiz:        "hkrpg_global",
		SignGame:       "hkrpg",
		DailyInfoURL:   "https://sg-public-api.hoyolab.com/event/luna/hkrpg/os/info?act_id=e202303301540311",
		RecordBaseURL:  "https://bbs-api-os.hoyolab.com/game_record/hkrpg/api",
		RecordEndpoint: "index",
		NotesEndpoint:  "note",
	},
}

type hoyoClient struct {
	httpClient *http.Client
	cookie     string
	userAgent  string
}

func main() {
	envPath := flag.String("env", defaultEnvPath, "dotenv file containing HOYOLAB_COOKIE")
	raw := flag.Bool("raw", false, "print complete game-record JSON responses")
	flag.Parse()

	if err := run(*envPath, *raw); err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: %v\n", err)
		os.Exit(1)
	}
}

func run(envPath string, printRaw bool) error {
	if err := loadDotEnv(envPath); err != nil {
		return err
	}

	cookie := strings.TrimSpace(os.Getenv("HOYOLAB_COOKIE"))
	if cookie == "" {
		ltuid := strings.TrimSpace(os.Getenv("HOYOLAB_LTUID_V2"))
		ltoken := strings.TrimSpace(os.Getenv("HOYOLAB_LTOKEN_V2"))
		if ltuid != "" && ltoken != "" {
			cookie = "ltuid_v2=" + ltuid + "; ltoken_v2=" + ltoken
		}
	}
	if err := validateCookie(cookie); err != nil {
		return err
	}

	userAgent := strings.TrimSpace(os.Getenv("HOYOLAB_USER_AGENT"))
	if userAgent == "" {
		userAgent = defaultUA
	}
	client := &hoyoClient{
		httpClient: &http.Client{
			Timeout: 20 * time.Second,
			CheckRedirect: func(_ *http.Request, _ []*http.Request) error {
				return http.ErrUseLastResponse
			},
		},
		cookie:    cookie,
		userAgent: userAgent,
	}

	ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
	defer cancel()

	fmt.Println("HoYoLAB Cookie認証とゲームアカウント一覧を確認します（Cookie値は出力しません）")
	accounts, err := client.getGameAccounts(ctx)
	if err != nil {
		return fmt.Errorf("ゲームアカウント一覧: %w", err)
	}
	if len(accounts) == 0 {
		return errors.New("Cookieは受理されましたが、連携済みゲームアカウントがありません")
	}

	fmt.Println("\n[連携アカウント]")
	for _, account := range accounts {
		fmt.Printf("- %s: UID=%s, server=%s, nickname=%s, level=%d\n",
			account.GameBiz, account.GameUID, account.Region, account.Nickname, account.Level)
	}

	accountByGame := make(map[string]gameAccount, len(accounts))
	for _, account := range accounts {
		current, exists := accountByGame[account.GameBiz]
		if !exists || account.Level > current.Level {
			accountByGame[account.GameBiz] = account
		}
	}

	var failures []error
	for _, game := range games {
		account, exists := accountByGame[game.GameBiz]
		if !exists {
			fmt.Printf("\n[%s]\nSKIP: このHoYoLABアカウントに%sの海外版アカウントが連携されていません\n", game.Name, game.Name)
			failures = append(failures, fmt.Errorf("%s: 連携済み海外版アカウントがありません", game.Name))
			continue
		}
		if err := client.checkGame(ctx, game, account, printRaw); err != nil {
			failures = append(failures, err)
		}
	}

	if len(failures) > 0 {
		return errors.Join(failures...)
	}
	fmt.Println("\nOK: 読み取り専用の全API確認が完了しました（ログインボーナスの受け取り操作はしていません）")
	return nil
}

func (c *hoyoClient) getGameAccounts(ctx context.Context) ([]gameAccount, error) {
	endpoint := "https://api-os-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie"
	data, err := c.get(ctx, endpoint, nil, map[string]string{
		"DS":                dynamicSecret(),
		"x-rpc-app_version": "1.5.0",
		"x-rpc-client_type": "5",
		"x-rpc-language":    defaultLang,
		"x-rpc-lang":        defaultLang,
	})
	if err != nil {
		return nil, err
	}

	var result gameAccountsData
	if err := json.Unmarshal(data, &result); err != nil {
		return nil, fmt.Errorf("レスポンスの解析: %w", err)
	}
	return result.List, nil
}

func (c *hoyoClient) checkGame(
	ctx context.Context,
	game gameSpec,
	account gameAccount,
	printRaw bool,
) error {
	fmt.Printf("\n[%s]\n", game.Name)
	var failures []error

	dailyData, err := c.get(ctx, game.DailyInfoURL, url.Values{"lang": {defaultLang}}, map[string]string{
		"Referer":        "https://act.hoyolab.com/",
		"x-rpc-signgame": game.SignGame,
	})
	if err != nil {
		failures = append(failures, fmt.Errorf("%s ログインボーナス状態: %w", game.Name, err))
		fmt.Printf("FAIL ログインボーナス状態: %v\n", err)
	} else {
		var info dailyInfo
		if err := json.Unmarshal(dailyData, &info); err != nil {
			failures = append(failures, fmt.Errorf("%s ログインボーナス状態の解析: %w", game.Name, err))
			fmt.Printf("FAIL ログインボーナス状態の解析: %v\n", err)
		} else {
			fmt.Printf("OK   ログインボーナス: 本日受取済み=%t, 今月の受取日数=%d\n",
				info.IsSign, info.TotalSignDay)
		}
	}

	query := url.Values{
		"role_id": {account.GameUID},
		"server":  {account.Region},
	}
	recordData, err := c.getRecord(ctx, game.RecordBaseURL+"/"+game.RecordEndpoint, query)
	if err != nil {
		failures = append(failures, fmt.Errorf("%s 基本ゲームレコード: %w", game.Name, err))
		fmt.Printf("FAIL 基本ゲームレコード: %v\n", err)
	} else {
		fmt.Printf("OK   基本ゲームレコード: %d bytes取得\n", len(recordData))
		if printRaw {
			printJSON("基本ゲームレコード", recordData)
		}
	}

	notesData, err := c.getRecord(ctx, game.RecordBaseURL+"/"+game.NotesEndpoint, query)
	if err != nil {
		failures = append(failures, fmt.Errorf("%s リアルタイム状態: %w", game.Name, err))
		fmt.Printf("FAIL リアルタイム状態: %v\n", err)
	} else {
		fmt.Printf("OK   リアルタイム状態: %d bytes取得\n", len(notesData))
		if printRaw {
			printJSON("リアルタイム状態", notesData)
		}
	}

	return errors.Join(failures...)
}

func (c *hoyoClient) getRecord(ctx context.Context, endpoint string, query url.Values) (json.RawMessage, error) {
	return c.get(ctx, endpoint, query, map[string]string{
		"DS":                dynamicSecret(),
		"x-rpc-app_version": "1.5.0",
		"x-rpc-client_type": "5",
		"x-rpc-language":    defaultLang,
		"x-rpc-lang":        defaultLang,
	})
}

func (c *hoyoClient) get(
	ctx context.Context,
	endpoint string,
	query url.Values,
	headers map[string]string,
) (json.RawMessage, error) {
	parsed, err := url.Parse(endpoint)
	if err != nil {
		return nil, fmt.Errorf("URLの解析: %w", err)
	}
	values := parsed.Query()
	for key, items := range query {
		for _, item := range items {
			values.Add(key, item)
		}
	}
	parsed.RawQuery = values.Encode()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		return nil, fmt.Errorf("リクエスト作成: %w", err)
	}
	req.Header.Set("Cookie", c.cookie)
	req.Header.Set("User-Agent", c.userAgent)
	for key, value := range headers {
		req.Header.Set(key, value)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("HTTPリクエスト: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(io.LimitReader(resp.Body, maxResponse+1))
	if err != nil {
		return nil, fmt.Errorf("レスポンス読み取り: %w", err)
	}
	if len(body) > maxResponse {
		return nil, fmt.Errorf("レスポンスが上限%d bytesを超えました", maxResponse)
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("HTTP %s", resp.Status)
	}

	var envelope apiEnvelope
	if err := json.Unmarshal(body, &envelope); err != nil {
		return nil, fmt.Errorf("JSONの解析: %w", err)
	}
	if envelope.Retcode != 0 {
		hint := ""
		switch envelope.Retcode {
		case -100, 10001, -1071:
			hint = "（Cookieが無効または期限切れです）"
		case 10101:
			hint = "（ゲームレコードの1日あたり参照上限の可能性があります）"
		case 10102:
			hint = "（HoYoLABで戦績またはリアルタイム状態の公開を有効にしてください）"
		}
		return nil, fmt.Errorf("HoYoLAB retcode=%d message=%q%s", envelope.Retcode, envelope.Message, hint)
	}
	if len(envelope.Data) == 0 || string(envelope.Data) == "null" {
		return nil, errors.New("HoYoLAB APIがdataを返しませんでした")
	}
	return envelope.Data, nil
}

func dynamicSecret() string {
	timestamp := time.Now().Unix()
	randomPart := randomLetters(6)
	input := fmt.Sprintf("salt=%s&t=%d&r=%s", overseasDSSalt, timestamp, randomPart)
	sum := md5.Sum([]byte(input))
	return fmt.Sprintf("%d,%s,%x", timestamp, randomPart, sum)
}

func randomLetters(length int) string {
	const alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
	result := make([]byte, length)
	randomBytes := make([]byte, length)
	if _, err := rand.Read(randomBytes); err != nil {
		// DS only needs a per-request nonce; a timestamp-derived fallback remains valid.
		randomBytes = []byte(strconv.FormatInt(time.Now().UnixNano(), 36))
	}
	for i := range result {
		result[i] = alphabet[int(randomBytes[i%len(randomBytes)])%len(alphabet)]
	}
	return string(result)
}

func validateCookie(raw string) error {
	if raw == "" {
		return errors.New("test/.env に HOYOLAB_COOKIE を設定してください")
	}
	if strings.ContainsAny(raw, "\r\n") {
		return errors.New("HOYOLAB_COOKIE に改行を含めることはできません")
	}

	req := &http.Request{Header: make(http.Header)}
	req.Header.Set("Cookie", raw)
	values := make(map[string]string)
	for _, cookie := range req.Cookies() {
		values[cookie.Name] = cookie.Value
	}

	uid := values["ltuid_v2"]
	if uid == "" {
		uid = values["account_id_v2"]
	}
	token := values["ltoken_v2"]
	if uid == "" || token == "" {
		legacyUID := values["ltuid"]
		if legacyUID == "" {
			legacyUID = values["account_id"]
		}
		if legacyUID == "" || values["ltoken"] == "" {
			return errors.New("HOYOLAB_COOKIE には少なくとも ltuid_v2 と ltoken_v2 が必要です")
		}
		uid = legacyUID
		token = values["ltoken"]
		fmt.Fprintln(os.Stderr, "WARN: 旧形式のltuid/ltokenを検出しました。現行のv2 Cookieを推奨します")
	}
	if strings.Contains(uid, "...") || strings.Contains(token, "...") ||
		strings.HasPrefix(uid, "YOUR_") || strings.HasPrefix(token, "YOUR_") {
		return errors.New("test/.env のプレースホルダーを実際のCookie値に置き換えてください")
	}
	return nil
}

func loadDotEnv(path string) error {
	file, err := os.Open(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("%s がありません。test/.env.example をコピーしてCookieを設定してください", path)
		}
		return fmt.Errorf("%s を開けません: %w", path, err)
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 1024), 1024*1024)
	for lineNumber := 1; scanner.Scan(); lineNumber++ {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		line = strings.TrimSpace(strings.TrimPrefix(line, "export "))
		key, value, found := strings.Cut(line, "=")
		if !found {
			return fmt.Errorf("%s:%d: KEY=VALUE 形式ではありません", path, lineNumber)
		}
		key = strings.TrimSpace(key)
		value = strings.TrimSpace(value)
		if key == "" {
			return fmt.Errorf("%s:%d: 環境変数名が空です", path, lineNumber)
		}
		if len(value) >= 2 && value[0] == '\'' && value[len(value)-1] == '\'' {
			value = value[1 : len(value)-1]
		} else if strings.HasPrefix(value, `"`) {
			unquoted, err := strconv.Unquote(value)
			if err != nil {
				return fmt.Errorf("%s:%d: ダブルクォートの解析: %w", path, lineNumber, err)
			}
			value = unquoted
		}
		if _, exists := os.LookupEnv(key); !exists {
			if err := os.Setenv(key, value); err != nil {
				return fmt.Errorf("%s:%d: 環境変数の設定: %w", path, lineNumber, err)
			}
		}
	}
	if err := scanner.Err(); err != nil {
		return fmt.Errorf("%s の読み取り: %w", path, err)
	}
	return nil
}

func printJSON(label string, raw json.RawMessage) {
	var pretty bytes.Buffer
	if err := json.Indent(&pretty, raw, "    ", "  "); err != nil {
		fmt.Printf("  %s: %s\n", label, raw)
		return
	}
	fmt.Printf("  %s:\n    %s\n", label, pretty.String())
}
