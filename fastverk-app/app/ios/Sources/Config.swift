// Config — the fixed endpoints the iOS console talks to.
//
// Auth matches the live WorkOS AuthKit setup used by web: hosted UI at
// login.fastverk.com, User Management API at id.fastverk.com, product origin
// app.fastverk.com. The AuthKit application is a public PKCE client (no secret
// ships in the app). Native redirect `fastverk://auth/callback` is already
// registered on that application alongside the web default
// `https://app.fastverk.com/auth/callback`.

import Foundation

enum Config {
    /// The web console origin. All /api/* calls are same-origin against this.
    static let appOrigin = URL(string: "https://app.fastverk.com")!

    /// AuthKit hosted UI (HostedAuthkit custom domain).
    static let authKitDomain = "login.fastverk.com"

    /// WorkOS Auth API (AuthAPI custom domain) — authorize + authenticate.
    static let authAPIDomain = "id.fastverk.com"

    /// Fastverk production AuthKit client id (public, PKCE).
    static let clientId = "client_01M0ZD0W24ZTHDBHBG0CSPPWHD"

    /// Native redirect (WorkOS AuthKit Redirects) + the scheme
    /// ASWebAuthenticationSession watches for.
    static let redirectURI = "fastverk://auth/callback"
    static let callbackScheme = "fastverk"

    /// AuthKit access tokens expire in 300s (application accessTokenExpiry).
    static let accessTokenLifetimeSeconds = 300

    static var authorizeURL: URL {
        URL(string: "https://\(authAPIDomain)/user_management/authorize")!
    }

    static var tokenURL: URL {
        URL(string: "https://\(authAPIDomain)/user_management/authenticate")!
    }
}
