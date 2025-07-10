def test_function():
    if True:
        for i in range(3):
            if i > 0:
                print(f"Bad indentation {i}")
                result = i * 2
                return result
    return None
