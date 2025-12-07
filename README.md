# WebHome

## About

This is a web home page that you can use as a home page for your browser.
Allowing you to rapidly access the information you need:

- Weather
- Notes
- Flux RSS
- Mails
- Links

You can also position freely any module everywhere you want on the page.

## Why

Remember [iGoogle](https://wikipedia.org/wiki/IGoogle) ?

after it was close, I wanted something like that made by myself.
I created the first version of this project in 2013, in PHP.
Now, I re-created it in Rust, using the [Leptos](https://leptos.dev/) framework.

## Security

This project uses a client system to only send crypted data to the server.
So anything stored on the server is encrypted and cannot be retrieved by anyone.

BUT, because of the internal restriction of browsers, the Mail and RSS modules need to send some information to the server :

- Flux RSS: Browser CORS disallow requesting data from another domain. So the server does this step itself and needs the full URL needed (no user information is used/logged).
- Mail: Browser cannot open IMAP connection directly. So the server does this step itself, and the full connection information (host/port/login/password) is used (no other user information is used, nothing from the user is logged).

Also, on the server, the site.json file configuration contains the server salt (that is never send to client).
If this salt is changed, any data stored on the server won't be accessible anymore.

## Todo

Something I want to add in the future:

- Calendar module
- Checklist module
- Password change (need client recrypt of all data)
- Layout system
- Design improvements

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Anything under the "static/public" dir is under private license. (unless stated otherwise)

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
