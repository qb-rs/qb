## qb-proto

This crate exposes primitives for instantiating and
maintaining a quixbyte protocol (QBP) connection. The QBP
is an upper application layer protocol, meaning it should be
used on top of some other protocol, which already ensures
that both peers are properly authenticated and that messages
can be sent reliably, so that there shall not be any man in
the middle attacks.

### Security

This protocol embarks no security meassures what so ever, as
it builds upon other protocols which shall implement these
safety meassures. Therefore the protocol itself is very safe
and reliant.

### Instantiation

The protocol's first step is to negotiate a common content type
and content encoding. It does this by sending a header packet,
which includes the magic bytes (b"QBP"), major and minor version,
as well as a header url search params string. The headers it sends
are for now "accept=application/json,application/bitcode,..." and
"accept-encoding=gzip,bzip2,...". By having access to both supported
content type and supported content encoding lists of both parties, we
can symmetrically negotiate the used parameters, which keeps us from
sending any other requests for manual negotiation. [See negotiation](#negotiation)

### Negotiation

We negotiate a common content type and content encoding by first finding
the viable canidates, that is, the shared content types and content encodings
in the lists of both parties, which we then rank by using the sum of the two
list indicies (the lower, the better). When we come uppon a pair with the same
sum, we choose the preferred method by sorting with the name of the content type or
encoding ("A..." is better than "Z..."). We can then just pick the best canidate
for content type and content encoding, which will then be used going forward.
