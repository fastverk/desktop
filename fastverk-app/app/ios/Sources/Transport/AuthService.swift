// AuthService — WorkOS AuthKit sign-in for the native console.
//
// Uses ASWebAuthenticationSession (system browser context, not an embedded
// WKWebView) so the OAuth hop works with Google federation and AuthKit
// passkeys (Google blocks embedded webviews). Authorization-code + PKCE
// against the Fastverk production AuthKit public client: authorize at
// id.fastverk.com (provider=authkit) lands on login.fastverk.com, then the
// callback `fastverk://auth/callback` is exchanged at
// /user_management/authenticate. Tokens in the Keychain; silent refresh is
// serialized through a single in-flight Task.

import AuthenticationServices
import CryptoKit
import Foundation
import SwiftUI

enum AuthError: Error, CustomStringConvertible {
    case badCallback
    case stateMismatch
    case tokenExchange(String)
    case needsLogin

    var description: String {
        switch self {
        case .badCallback: return "The sign-in response was malformed."
        case .stateMismatch: return "Sign-in state mismatch; please try again."
        case let .tokenExchange(m): return "Token exchange failed: \(m)"
        case .needsLogin: return "Your session expired; please sign in again."
        }
    }
}

@MainActor
final class AuthService: ObservableObject {
    @Published private(set) var isAuthenticated = false

    private var accessToken: String?
    private var refreshToken: String?
    private var expiresAt: Date?

    private var authSession: ASWebAuthenticationSession?
    private let anchorProvider = PresentationAnchorProvider()
    private var refreshTask: Task<String, Error>?

    private let session = URLSession(configuration: .ephemeral)

    // MARK: - Session lifecycle

    /// Restore tokens from the Keychain on launch. If the access token is still
    /// valid we're signed in immediately; if only a refresh_token survives we
    /// try a silent refresh. Leftover Cognito `id_token` keys are dropped so a
    /// pre-cutover session cannot be sent to AuthKit-backed APIs.
    func restoreSession() async {
        Keychain.delete("id_token")
        accessToken = Keychain.get("access_token")
        refreshToken = Keychain.get("refresh_token")
        if let s = Keychain.get("expires_at"), let t = TimeInterval(s) {
            expiresAt = Date(timeIntervalSince1970: t)
        }
        if let exp = expiresAt, exp > Date(), accessToken != nil {
            isAuthenticated = true
            return
        }
        if refreshToken != nil {
            _ = try? await validIdToken()
        }
    }

    func signOut() {
        accessToken = nil
        refreshToken = nil
        expiresAt = nil
        Keychain.delete("access_token")
        Keychain.delete("id_token")
        Keychain.delete("refresh_token")
        Keychain.delete("expires_at")
        isAuthenticated = false
    }

    // MARK: - Token access (for ShellClient / NetworkRpcInvoker)

    /// A currently-valid AuthKit access token, refreshing first if it's
    /// expired/near-expiry. Concurrent callers share one refresh.
    func validIdToken() async throws -> String {
        if let tok = accessToken, let exp = expiresAt, exp > Date() {
            return tok
        }
        return try await refresh()
    }

    /// Force a refresh (used on a 401 even if the local clock thinks it's valid).
    func refreshedIdToken() async throws -> String {
        try await refresh()
    }

    // MARK: - Sign-in flow

    func signIn() async throws {
        let verifier = Self.randomURLSafe(count: 64)
        let challenge = Self.codeChallenge(for: verifier)
        let state = Self.randomURLSafe(count: 32)

        var comps = URLComponents(url: Config.authorizeURL, resolvingAgainstBaseURL: false)!
        comps.queryItems = [
            .init(name: "response_type", value: "code"),
            .init(name: "client_id", value: Config.clientId),
            .init(name: "redirect_uri", value: Config.redirectURI),
            .init(name: "provider", value: "authkit"),
            .init(name: "state", value: state),
            .init(name: "code_challenge", value: challenge),
            .init(name: "code_challenge_method", value: "S256"),
        ]

        let callback: URL = try await withCheckedThrowingContinuation { cont in
            let s = ASWebAuthenticationSession(
                url: comps.url!,
                callbackURLScheme: Config.callbackScheme
            ) { url, error in
                if let error {
                    if (error as? ASWebAuthenticationSessionError)?.code == .canceledLogin {
                        cont.resume(throwing: CancellationError())
                    } else {
                        cont.resume(throwing: error)
                    }
                    return
                }
                guard let url else { cont.resume(throwing: AuthError.badCallback); return }
                cont.resume(returning: url)
            }
            s.presentationContextProvider = anchorProvider
            // Share the system Safari cookie jar so AuthKit passkeys / Google
            // SSO match the web session on login.fastverk.com.
            s.prefersEphemeralWebBrowserSession = false
            authSession = s
            if !s.start() {
                cont.resume(throwing: AuthError.badCallback)
            }
        }
        authSession = nil

        let items = URLComponents(url: callback, resolvingAgainstBaseURL: false)?.queryItems ?? []
        guard items.first(where: { $0.name == "state" })?.value == state else {
            throw AuthError.stateMismatch
        }
        guard let code = items.first(where: { $0.name == "code" })?.value else {
            throw AuthError.badCallback
        }
        try await exchange(grant: [
            "grant_type": "authorization_code",
            "client_id": Config.clientId,
            "code": code,
            "code_verifier": verifier,
        ])
    }

    // MARK: - Token endpoint

    private func refresh() async throws -> String {
        if let inFlight = refreshTask {
            return try await inFlight.value
        }
        guard let rt = refreshToken else {
            isAuthenticated = false
            throw AuthError.needsLogin
        }
        let task = Task { () throws -> String in
            try await self.exchange(grant: [
                "grant_type": "refresh_token",
                "client_id": Config.clientId,
                "refresh_token": rt,
            ])
            guard let tok = self.accessToken else { throw AuthError.needsLogin }
            return tok
        }
        refreshTask = task
        defer { refreshTask = nil }
        do {
            return try await task.value
        } catch {
            // A rejected refresh token means the session is gone.
            signOut()
            throw AuthError.needsLogin
        }
    }

    /// POST the form grant to AuthKit's /user_management/authenticate and store
    /// the access + refresh tokens. Public clients send PKCE (`code_verifier`)
    /// and no client secret.
    private func exchange(grant: [String: String]) async throws {
        var req = URLRequest(url: Config.tokenURL)
        req.httpMethod = "POST"
        req.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        req.httpBody = Self.formEncode(grant).data(using: .utf8)

        let (data, resp) = try await session.data(for: req)
        guard let http = resp as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            let body = String(decoding: data, as: UTF8.self)
            throw AuthError.tokenExchange(body.isEmpty ? "HTTP error" : body)
        }
        let token = try JSONDecoder().decode(TokenResponse.self, from: data)
        accessToken = token.access_token
        if let rt = token.refresh_token { refreshToken = rt } // refresh grant may omit it
        let lifetime = TimeInterval(token.expires_in ?? Config.accessTokenLifetimeSeconds)
        expiresAt = Date().addingTimeInterval(lifetime - 60) // 60s skew

        Keychain.set(accessToken ?? "", for: "access_token")
        if let rt = refreshToken { Keychain.set(rt, for: "refresh_token") }
        Keychain.set(String(expiresAt!.timeIntervalSince1970), for: "expires_at")
        isAuthenticated = true
    }

    private struct TokenResponse: Decodable {
        let access_token: String
        let refresh_token: String?
        let expires_in: Int?
    }

    // MARK: - PKCE + form helpers

    private static func randomURLSafe(count: Int) -> String {
        var bytes = [UInt8](repeating: 0, count: count)
        _ = SecRandomCopyBytes(kSecRandomDefault, count, &bytes)
        return base64URL(Data(bytes))
    }

    private static func codeChallenge(for verifier: String) -> String {
        let digest = SHA256.hash(data: Data(verifier.utf8))
        return base64URL(Data(digest))
    }

    private static func base64URL(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private static func formEncode(_ params: [String: String]) -> String {
        var allowed = CharacterSet.alphanumerics
        allowed.insert(charactersIn: "-._~")
        return params.map { key, value in
            let k = key.addingPercentEncoding(withAllowedCharacters: allowed) ?? key
            let v = value.addingPercentEncoding(withAllowedCharacters: allowed) ?? value
            return "\(k)=\(v)"
        }.joined(separator: "&")
    }
}

/// Supplies the window ASWebAuthenticationSession presents over.
private final class PresentationAnchorProvider: NSObject, ASWebAuthenticationPresentationContextProviding {
    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        return scene?.keyWindow ?? ASPresentationAnchor()
    }
}
