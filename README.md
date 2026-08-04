# server

A zero knowledge server for self-hostable + secure storage platform. 
<hr>

> [!NOTE]
> The zero-knowledge architecture is the long-term target. 
> Client-side encryption and certificate pinning (and the client itself) are planned but not yet implemented. 
> To see features under development see [issues](https://github.com/kosh-project/server/issues)



<h2> Features </h2>
<ol>
      <li>
            <b>Zero Knowledge Host</b>
            <p>The server knows nothing that could identify</p>
            <ul>
                  <li>The user or their client</li>
                  <li>Any file stored by the user. Server only stores <code>file-size</code>, <code>hash</code> and the <code>user</code> who owns it.</li>
                  <li>Every file uploaded is client-side encrypted — including its MIME type, metadata, and content.</li>
            </ul>
      </li>
      <li>
            <b>Certificate Pinning *</b>
            <p>
                  All connections use <code>https</code> with a self-signed <code>X.509 certificate</code> generated on first boot.
                  The certificate fingerprint is verified physically — out-of-band — before any connection is established. No certificate authority needed.
                  And this all happens on a <code>localhost</code>ed network.
            </p>
      </li>
      <li>
            <b>Low-Resource Design</b>
            <p>
                Kosh is built to run well on hardware most people have already written off.
                Reference deployment targets:
                <ul>
                    <li>A <a href="https://www.intel.com/content/www/us/en/products/sku/52207/intel-core-i52400-processor-6m-cache-up-to-3-40-ghz/specifications.html">2nd-gen Intel Sandy Bridge</a> box with 2GBs of RAM</li>
                    <li>A <a href='https://www.hp.com/us-en/shop/pdp/hp-chromebook-11a-na0060nr'>Chromebook</a> with <a href='https://www.mediatek.com/products/tablets/mt8183'>MediaTek MT8183</a> and 4GBs of RAM</li>
                </ul>
                If it's fast there, it's fast anywhere.
            </p> 
      </li>
</ol>

\* - Planned. May change as the project evolves.