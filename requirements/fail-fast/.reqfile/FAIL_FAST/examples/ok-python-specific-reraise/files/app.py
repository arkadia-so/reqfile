def setting(config, key):
    try:
        return config[key]
    except KeyError as missing:
        raise ConfigError(f"missing setting {key}") from missing
